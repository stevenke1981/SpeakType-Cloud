use crate::error::{AppError, AppResult};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::io::Cursor;
use std::ops::DerefMut;
use std::sync::{Arc, Mutex};

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
        let mono = if self.channels <= 1 {
            self.samples.clone()
        } else {
            self.samples
                .chunks(self.channels as usize)
                .map(|frame| frame.iter().copied().sum::<f32>() / frame.len() as f32)
                .collect()
        };
        resample_linear(&mono, self.sample_rate, 16_000)
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
}

pub struct Recorder {
    host: cpal::Host,
    device_name: Option<String>,
    stream: Option<cpal::Stream>,
    buffer: Arc<Mutex<Vec<f32>>>,
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
            buffer: Arc::new(Mutex::new(Vec::new())),
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
        if self.stream.is_some() {
            return Ok(());
        }
        if let Ok(mut buffer) = self.buffer.lock() {
            buffer.clear();
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
                    move |data: &[f32], _| push_samples(&buffer, data.iter().copied(), gain),
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
                            data.iter().map(|s| *s as f32 / i16::MAX as f32),
                            gain,
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
                            data.iter()
                                .map(|s| (*s as f32 / u16::MAX as f32) * 2.0 - 1.0),
                            gain,
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
        let mut samples = std::mem::take(guard.deref_mut());
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

fn push_samples(buffer: &Arc<Mutex<Vec<f32>>>, samples: impl Iterator<Item = f32>, gain: f32) {
    if let Ok(mut buffer) = buffer.lock() {
        buffer.extend(samples.map(|sample| (sample * gain).clamp(-1.0, 1.0)));
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
}
