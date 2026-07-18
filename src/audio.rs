use crate::config::MAX_RECORDING_DURATION_SECS;
use crate::error::{AppError, AppResult};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_queue::ArrayQueue;
use std::io::Cursor;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

const MAX_LIVE_CHUNK_SAMPLES: usize = 8_192;

#[derive(Clone, Debug)]
pub struct RecordedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl RecordedAudio {
    pub fn duration_secs(&self) -> f32 {
        if self.sample_rate == 0 || self.channels == 0 {
            return 0.0;
        }
        self.samples.len() as f32 / self.sample_rate as f32 / self.channels as f32
    }

    pub fn mono_16khz(&self) -> Vec<f32> {
        self.mono_resampled(16_000)
    }

    pub fn mono_resampled(&self, target_rate: u32) -> Vec<f32> {
        let mono = if self.channels <= 1 {
            self.samples.clone()
        } else {
            self.samples
                .chunks(self.channels as usize)
                .map(|frame| frame.iter().copied().sum::<f32>() / frame.len() as f32)
                .collect()
        };
        resample_linear(&mono, self.sample_rate, target_rate)
    }

    pub fn wav_16khz_mono_i16(&self) -> AppResult<Vec<u8>> {
        let mono = self.mono_16khz();
        let mut cursor = Cursor::new(Vec::<u8>::new());
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        {
            let mut writer = hound::WavWriter::new(&mut cursor, spec)
                .map_err(|e| AppError::Audio(e.to_string()))?;
            for sample in mono {
                let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                writer
                    .write_sample(value)
                    .map_err(|e| AppError::Audio(e.to_string()))?;
            }
            writer
                .finalize()
                .map_err(|e| AppError::Audio(e.to_string()))?;
        }
        Ok(cursor.into_inner())
    }

    #[cfg(test)]
    pub fn pcm16_mono(&self, target_rate: u32) -> Vec<i16> {
        self.mono_resampled(target_rate)
            .into_iter()
            .map(|sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
            .collect()
    }
}

#[derive(Debug)]
pub struct LiveAudioChunk {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    recycle: Option<Arc<ArrayQueue<Vec<f32>>>>,
}

impl LiveAudioChunk {
    #[cfg(test)]
    pub(crate) fn from_samples(samples: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            samples,
            sample_rate,
            recycle: None,
        }
    }

    #[cfg(test)]
    fn for_test(samples: Vec<f32>, sample_rate: u32) -> Self {
        Self::from_samples(samples, sample_rate)
    }
}

impl Drop for LiveAudioChunk {
    fn drop(&mut self) {
        let Some(pool) = &self.recycle else {
            return;
        };
        let mut samples = std::mem::take(&mut self.samples);
        samples.clear();
        let _ = pool.push(samples);
    }
}

#[derive(Clone, Default)]
pub struct LiveAudioStats {
    dropped_chunks: Arc<AtomicU64>,
    dropped_capture_samples: Arc<AtomicU64>,
}

impl LiveAudioStats {
    pub fn dropped_chunks(&self) -> u64 {
        self.dropped_chunks.load(Ordering::Relaxed)
    }

    pub fn dropped_capture_samples(&self) -> u64 {
        self.dropped_capture_samples.load(Ordering::Relaxed)
    }
}

#[derive(Clone)]
pub struct LiveAudioSink {
    sender: SyncSender<LiveAudioChunk>,
    stats: LiveAudioStats,
    pool: Arc<ArrayQueue<Vec<f32>>>,
}

impl LiveAudioSink {
    #[cfg(test)]
    pub fn try_send(&self, chunk: LiveAudioChunk) -> bool {
        match self.sender.try_send(chunk) {
            Ok(()) => true,
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                self.stats.dropped_chunks.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }

    fn acquire_buffer(&self) -> Option<Vec<f32>> {
        self.pool.pop()
    }

    fn submit_buffer(&self, samples: Vec<f32>, sample_rate: u32) {
        if samples.is_empty() {
            let _ = self.pool.push(samples);
            return;
        }
        let chunk = LiveAudioChunk {
            samples,
            sample_rate,
            recycle: Some(Arc::clone(&self.pool)),
        };
        if let Err(TrySendError::Full(mut chunk) | TrySendError::Disconnected(mut chunk)) =
            self.sender.try_send(chunk)
        {
            self.stats.dropped_chunks.fetch_add(1, Ordering::Relaxed);
            chunk.recycle = None;
            let mut samples = std::mem::take(&mut chunk.samples);
            samples.clear();
            let _ = self.pool.push(samples);
        }
    }
}

pub fn bounded_live_audio(
    capacity: usize,
) -> (LiveAudioSink, Receiver<LiveAudioChunk>, LiveAudioStats) {
    let (sender, receiver) = mpsc::sync_channel(capacity.max(1));
    let stats = LiveAudioStats::default();
    let pool_capacity = capacity.max(1) + 1;
    let pool = Arc::new(ArrayQueue::new(pool_capacity));
    for _ in 0..pool_capacity {
        let _ = pool.push(Vec::with_capacity(MAX_LIVE_CHUNK_SAMPLES));
    }
    (
        LiveAudioSink {
            sender,
            stats: stats.clone(),
            pool,
        },
        receiver,
        stats,
    )
}

struct CaptureBuffer {
    samples: Vec<f32>,
    write_index: usize,
    len: usize,
    overwritten: u64,
}

impl CaptureBuffer {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            samples: vec![0.0; capacity],
            write_index: 0,
            len: 0,
            overwritten: 0,
        }
    }

    fn push(&mut self, sample: f32) -> bool {
        if self.samples.is_empty() {
            self.overwritten = self.overwritten.saturating_add(1);
            return true;
        }
        let overwritten = self.len == self.samples.len();
        self.samples[self.write_index] = sample;
        self.write_index = (self.write_index + 1) % self.samples.len();
        if overwritten {
            self.overwritten = self.overwritten.saturating_add(1);
        } else {
            self.len += 1;
        }
        overwritten
    }

    fn ordered_samples(&self) -> Vec<f32> {
        if self.len < self.samples.len() {
            return self.samples[..self.len].to_vec();
        }
        self.samples[self.write_index..]
            .iter()
            .chain(&self.samples[..self.write_index])
            .copied()
            .collect()
    }

    fn take_ordered(&mut self) -> Vec<f32> {
        let ordered = self.ordered_samples();
        self.write_index = 0;
        self.len = 0;
        self.overwritten = 0;
        ordered
    }

    #[cfg(test)]
    fn storage_capacity(&self) -> usize {
        self.samples.len()
    }

    #[cfg(test)]
    fn overwritten_samples(&self) -> u64 {
        self.overwritten
    }
}

pub struct Recorder {
    host: cpal::Host,
    device_name: Option<String>,
    stream: Option<cpal::Stream>,
    buffer: Arc<Mutex<CaptureBuffer>>,
    stream_error: Arc<Mutex<Option<String>>>,
    sample_rate: u32,
    channels: u16,
    gain: f32,
}

impl Recorder {
    pub fn new(device_name: Option<String>, gain: f32) -> Self {
        Self {
            host: cpal::default_host(),
            device_name,
            stream: None,
            buffer: Arc::new(Mutex::new(CaptureBuffer::with_capacity(0))),
            stream_error: Arc::new(Mutex::new(None)),
            sample_rate: 16_000,
            channels: 1,
            gain: gain.max(0.1),
        }
    }

    pub fn list_devices(&self) -> Vec<String> {
        self.host
            .input_devices()
            .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
            .unwrap_or_default()
    }

    pub fn update_config(&mut self, device_name: Option<String>, gain: f32) {
        self.device_name = device_name;
        self.gain = gain.max(0.1);
    }

    pub fn start(&mut self) -> AppResult<()> {
        self.start_with_live_sink(None)
    }

    pub fn start_live(
        &mut self,
        capacity: usize,
    ) -> AppResult<(Receiver<LiveAudioChunk>, LiveAudioStats)> {
        let (sink, receiver, stats) = bounded_live_audio(capacity);
        self.start_with_live_sink(Some(sink))?;
        Ok((receiver, stats))
    }

    fn start_with_live_sink(&mut self, live_sink: Option<LiveAudioSink>) -> AppResult<()> {
        if self.stream.is_some() {
            return Err(AppError::Audio("麥克風已在錄音中".to_string()));
        }
        if let Ok(mut error) = self.stream_error.lock() {
            *error = None;
        }
        let device = find_device(&self.host, self.device_name.as_deref())
            .ok_or_else(|| AppError::Audio("找不到麥克風輸入裝置".to_string()))?;
        let config = device
            .default_input_config()
            .map_err(|e| AppError::Audio(e.to_string()))?;
        self.sample_rate = config.sample_rate().0;
        self.channels = config.channels();
        let capture_capacity = self.sample_rate as usize
            * usize::from(self.channels.max(1))
            * MAX_RECORDING_DURATION_SECS as usize;
        if let Ok(mut buffer) = self.buffer.lock() {
            *buffer = CaptureBuffer::with_capacity(capture_capacity);
        }
        let sample_rate = self.sample_rate;
        let channels = self.channels;
        let buffer = self.buffer.clone();
        let stream_error = self.stream_error.clone();
        let gain = self.gain;
        let err_fn = move |error: cpal::StreamError| {
            store_stream_error(&stream_error, &error.to_string());
        };

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[f32], _| {
                        push_samples(
                            &buffer,
                            live_sink.as_ref(),
                            data.iter().copied(),
                            gain,
                            channels,
                            sample_rate,
                        )
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| AppError::Audio(e.to_string()))?,
            cpal::SampleFormat::I16 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[i16], _| {
                        push_samples(
                            &buffer,
                            live_sink.as_ref(),
                            data.iter().map(|s| *s as f32 / i16::MAX as f32),
                            gain,
                            channels,
                            sample_rate,
                        )
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| AppError::Audio(e.to_string()))?,
            cpal::SampleFormat::U16 => device
                .build_input_stream(
                    &config.into(),
                    move |data: &[u16], _| {
                        push_samples(
                            &buffer,
                            live_sink.as_ref(),
                            data.iter()
                                .map(|s| (*s as f32 / u16::MAX as f32) * 2.0 - 1.0),
                            gain,
                            channels,
                            sample_rate,
                        )
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| AppError::Audio(e.to_string()))?,
            other => return Err(AppError::Audio(format!("不支援的取樣格式：{other:?}"))),
        };
        stream.play().map_err(|e| AppError::Audio(e.to_string()))?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) -> RecordedAudio {
        self.stream.take();
        let mut guard = match self.buffer.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut samples = guard.take_ordered();
        normalize(&mut samples);
        RecordedAudio {
            samples,
            sample_rate: self.sample_rate,
            channels: self.channels,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.stream.is_some()
    }

    pub fn take_stream_error(&self) -> Option<AppError> {
        self.stream_error
            .lock()
            .ok()?
            .take()
            .map(|message| AppError::Audio(format!("麥克風串流中斷：{message}")))
    }
}

fn push_samples(
    buffer: &Arc<Mutex<CaptureBuffer>>,
    live_sink: Option<&LiveAudioSink>,
    samples: impl Iterator<Item = f32>,
    gain: f32,
    channels: u16,
    sample_rate: u32,
) {
    let mut capture = buffer.try_lock().ok();
    let mut mono = live_sink.and_then(LiveAudioSink::acquire_buffer);
    let channel_count = usize::from(channels.max(1));
    let mut channel_sum = 0.0;
    let mut channel_index = 0;
    for sample in samples {
        let sample = (sample * gain).clamp(-1.0, 1.0);
        if let Some(capture) = capture.as_mut() {
            if capture.push(sample) {
                if let Some(sink) = live_sink {
                    sink.stats
                        .dropped_capture_samples
                        .fetch_add(1, Ordering::Relaxed);
                }
            }
        } else if let Some(sink) = live_sink {
            sink.stats
                .dropped_capture_samples
                .fetch_add(1, Ordering::Relaxed);
        }
        channel_sum += sample;
        channel_index += 1;
        if channel_index == channel_count {
            if let Some(mono) = mono.as_mut() {
                if mono.len() < mono.capacity() {
                    mono.push(channel_sum / channel_count as f32);
                } else if let Some(sink) = live_sink {
                    sink.stats.dropped_chunks.fetch_add(1, Ordering::Relaxed);
                }
            }
            channel_sum = 0.0;
            channel_index = 0;
        }
    }
    drop(capture);
    if let (Some(sink), Some(mono)) = (live_sink, mono) {
        sink.submit_buffer(mono, sample_rate);
    }
}

fn store_stream_error(slot: &Arc<Mutex<Option<String>>>, message: &str) {
    if let Ok(mut slot) = slot.lock() {
        *slot = Some(message.to_string());
    }
}

fn find_device(host: &cpal::Host, requested: Option<&str>) -> Option<cpal::Device> {
    if let Some(requested) = requested.filter(|value| !value.trim().is_empty()) {
        if let Ok(mut devices) = host.input_devices() {
            if let Some(device) =
                devices.find(|device| device.name().ok().as_deref() == Some(requested))
            {
                return Some(device);
            }
        }
    }
    host.default_input_device()
}

fn normalize(samples: &mut [f32]) {
    if samples.is_empty() {
        return;
    }
    let rms = (samples.iter().map(|v| v * v).sum::<f32>() / samples.len() as f32).sqrt();
    if rms < 0.001 {
        return;
    }
    let gain = (0.20 / rms).clamp(0.25, 3.0);
    for sample in samples {
        *sample = (*sample * gain).clamp(-1.0, 1.0);
    }
}

fn resample_linear(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if samples.is_empty() || from_rate == 0 || from_rate == to_rate {
        return samples.to_vec();
    }
    let output_len = (samples.len() as u64 * to_rate as u64 / from_rate as u64).max(1) as usize;
    let ratio = from_rate as f64 / to_rate as f64;
    let mut output = Vec::with_capacity(output_len);
    for index in 0..output_len {
        let position = index as f64 * ratio;
        let left = position.floor() as usize;
        let right = (left + 1).min(samples.len() - 1);
        let fraction = (position - left as f64) as f32;
        output.push(samples[left] * (1.0 - fraction) + samples[right] * fraction);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_audio_becomes_mono_16khz() {
        let audio = RecordedAudio {
            samples: vec![1.0, -1.0, 0.5, 0.5],
            sample_rate: 16_000,
            channels: 2,
        };
        assert_eq!(audio.mono_16khz(), vec![0.0, 0.5]);
    }

    #[test]
    fn wav_has_riff_header() {
        let audio = RecordedAudio {
            samples: vec![0.0; 160],
            sample_rate: 16_000,
            channels: 1,
        };
        let wav = audio.wav_16khz_mono_i16().expect("wav");
        assert_eq!(&wav[..4], b"RIFF");
    }

    #[test]
    fn stream_error_is_buffered_for_the_ui_thread() {
        let slot = Arc::new(Mutex::new(None));

        store_stream_error(&slot, "device disconnected");

        assert_eq!(
            slot.lock().expect("error slot").as_deref(),
            Some("device disconnected")
        );
    }

    #[test]
    fn realtime_pcm_is_resampled_for_each_provider() {
        let audio = RecordedAudio {
            samples: vec![0.25; 480],
            sample_rate: 48_000,
            channels: 1,
        };
        assert_eq!(audio.pcm16_mono(16_000).len(), 160);
        assert_eq!(audio.pcm16_mono(24_000).len(), 240);
    }

    #[test]
    fn bounded_live_sink_never_blocks_and_reports_drops() {
        let (sink, receiver, stats) = bounded_live_audio(1);
        assert!(sink.try_send(LiveAudioChunk::for_test(vec![0.1], 48_000)));
        assert!(!sink.try_send(LiveAudioChunk::for_test(vec![0.2], 48_000)));
        assert_eq!(stats.dropped_chunks(), 1);
        assert_eq!(receiver.try_recv().expect("first chunk").samples, vec![0.1]);
    }

    #[test]
    fn callback_path_converts_to_mono_and_applies_gain_before_try_send() {
        let buffer = Arc::new(Mutex::new(CaptureBuffer::with_capacity(8)));
        let (sink, receiver, _) = bounded_live_audio(1);
        push_samples(
            &buffer,
            Some(&sink),
            [0.25, -0.25, 0.5, 0.5].into_iter(),
            2.0,
            2,
            48_000,
        );
        let chunk = receiver.try_recv().expect("live chunk");
        assert_eq!(chunk.samples, vec![0.0, 1.0]);
        assert_eq!(chunk.sample_rate, 48_000);
        assert_eq!(
            buffer.lock().expect("buffer").ordered_samples(),
            vec![0.5, -0.5, 1.0, 1.0]
        );
        assert_eq!(chunk.samples.capacity(), MAX_LIVE_CHUNK_SAMPLES);
    }

    #[test]
    fn callback_capture_ring_has_fixed_capacity_and_reports_overwrite() {
        let mut buffer = CaptureBuffer::with_capacity(4);
        let original_capacity = buffer.storage_capacity();
        for sample in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0] {
            buffer.push(sample);
        }
        assert_eq!(buffer.storage_capacity(), original_capacity);
        assert_eq!(original_capacity, 4);
        assert_eq!(buffer.ordered_samples(), vec![3.0, 4.0, 5.0, 6.0]);
        assert_eq!(buffer.overwritten_samples(), 2);
    }
}
