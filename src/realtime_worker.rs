use crate::audio::LiveAudioChunk;
#[cfg(test)]
use crate::audio::RecordedAudio;
use crate::config::{AppConfig, ProviderKind, TranscriptionMode};
use crate::providers::load_api_key;
use crate::realtime::{self, RealtimeEvent, RealtimeSession};
use crate::vad::{ClientVad, VadConfig, VadEvent};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

pub enum WorkerMessage {
    Event(RealtimeEvent),
    Failed(String),
    Stopped,
}

enum Control {
    Finalize,
    Cancel,
}

pub struct RealtimeWorkerHandle {
    control: SyncSender<Control>,
    cancelled: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl RealtimeWorkerHandle {
    pub fn finalize(&self) {
        let _ = self.control.try_send(Control::Finalize);
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        let _ = self.control.try_send(Control::Cancel);
    }

    #[cfg(test)]
    pub fn is_finished(&self) -> bool {
        self.join
            .as_ref()
            .is_none_or(std::thread::JoinHandle::is_finished)
    }

    #[cfg(test)]
    pub fn join_if_finished(&mut self) -> bool {
        if !self.is_finished() {
            return false;
        }
        self.join.take().is_none_or(|join| join.join().is_ok())
    }

    pub fn join_after_ack(&mut self) {
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }

    #[cfg(test)]
    fn from_parts_for_test(
        control: SyncSender<Control>,
        cancelled: Arc<AtomicBool>,
        join: std::thread::JoinHandle<()>,
    ) -> Self {
        Self {
            control,
            cancelled,
            join: Some(join),
        }
    }
}

impl Drop for RealtimeWorkerHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}

pub fn spawn(
    config: AppConfig,
    audio: Receiver<LiveAudioChunk>,
    output: mpsc::Sender<WorkerMessage>,
) -> RealtimeWorkerHandle {
    let (control_tx, control_rx) = mpsc::sync_channel(4);
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = Arc::clone(&cancelled);
    let join = std::thread::spawn(move || {
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())
            .and_then(|runtime| {
                runtime.block_on(run(config, audio, control_rx, &worker_cancelled, &output))
            });
        if let Err(error) = result {
            let _ = output.send(WorkerMessage::Failed(error));
        }
        let _ = output.send(WorkerMessage::Stopped);
    });
    RealtimeWorkerHandle {
        control: control_tx,
        cancelled,
        join: Some(join),
    }
}

async fn run(
    config: AppConfig,
    audio: Receiver<LiveAudioChunk>,
    control: Receiver<Control>,
    cancelled: &AtomicBool,
    output: &mpsc::Sender<WorkerMessage>,
) -> Result<(), String> {
    let api_key = load_api_key(&config).map_err(|error| error.to_string())?;
    let connection = async {
        match config.provider {
            ProviderKind::OpenAi => Ok::<(RealtimeSession, u32, usize), String>((
                realtime::openai::connect(
                    realtime::openai::OFFICIAL_ENDPOINT,
                    api_key,
                    config.realtime.openai_model.clone(),
                    config.language.clone(),
                    config.realtime.openai_transcription_delay,
                )
                .await
                .map_err(|error| error.to_string())?,
                realtime::openai::SAMPLE_RATE,
                2_400,
            )),
            ProviderKind::Xai => Ok::<(RealtimeSession, u32, usize), String>((
                realtime::xai::connect(
                    realtime::xai::OFFICIAL_ENDPOINT,
                    api_key,
                    config.realtime.xai_smart_turn_enabled,
                    config.realtime.xai_smart_turn_threshold,
                    config.realtime.xai_smart_turn_timeout_ms,
                )
                .await
                .map_err(|error| error.to_string())?,
                realtime::xai::SAMPLE_RATE,
                1_600,
            )),
        }
    };
    let (mut session, sample_rate, frame_samples) = tokio::select! {
        result = tokio::time::timeout(Duration::from_secs(15), connection) => {
            result.map_err(|_| "Realtime 連線逾時".to_string())??
        }
        _ = wait_for_cancellation(cancelled) => return Ok(()),
    };
    let local_vad = config.transcription_mode == TranscriptionMode::ContinuousDictation
        && (config.provider == ProviderKind::OpenAi || !config.realtime.xai_smart_turn_enabled);
    let frame_ms = 10_u64;
    let mut vad = ClientVad::new(VadConfig {
        rms_threshold: config.realtime.vad_rms_threshold,
        pre_roll_frames: frames(config.realtime.vad_pre_roll_ms, frame_ms),
        min_speech_frames: frames(config.realtime.vad_min_speech_ms, frame_ms),
        silence_frames: frames(config.realtime.vad_silence_ms, frame_ms),
        max_utterance_frames: frames(config.realtime.max_utterance_secs * 1_000, frame_ms),
    });
    let mut converter = StreamingAudioConverter::new(sample_rate);

    let mut finalize_requested = false;
    loop {
        if cancelled.load(Ordering::Acquire) {
            let _ = session.cancel().await;
            return Ok(());
        }
        match control.try_recv() {
            Ok(Control::Finalize) => finalize_requested = true,
            Ok(Control::Cancel) => {
                let _ = session.cancel().await;
                return Ok(());
            }
            Err(TryRecvError::Disconnected) => {
                let _ = session.cancel().await;
                return Ok(());
            }
            Err(TryRecvError::Empty) => {}
        }
        let drained = drain_ready_audio(&audio);
        for chunk in drained.chunks {
            let frames = converter.push(&chunk.samples, chunk.sample_rate)?;
            for frame in frames {
                send_stream_frame(&mut session, &mut vad, local_vad, frame, frame_samples).await?;
            }
        }
        if finalize_requested && drained.producer_closed {
            for frame in converter.finish() {
                send_stream_frame(&mut session, &mut vad, local_vad, frame, frame_samples).await?;
            }
            session
                .finalize()
                .await
                .map_err(|error| error.to_string())?;
            finalize_requested = false;
        }
        loop {
            match session
                .try_next_event()
                .map_err(|error| error.to_string())?
            {
                Some(RealtimeEvent::Error(error)) => return Err(error),
                Some(event) => {
                    let _ = output.send(WorkerMessage::Event(event));
                }
                None => break,
            }
        }
        tokio::time::sleep(Duration::from_millis(8)).await;
    }
}

struct StreamingAudioConverter {
    target_rate: u32,
    source_rate: Option<u32>,
    source: VecDeque<f32>,
    source_base: u64,
    source_total: u64,
    output_index: u64,
    frame_samples: usize,
    frame_carry: VecDeque<f32>,
}

impl StreamingAudioConverter {
    fn new(target_rate: u32) -> Self {
        Self {
            target_rate,
            source_rate: None,
            source: VecDeque::new(),
            source_base: 0,
            source_total: 0,
            output_index: 0,
            frame_samples: (target_rate / 100).max(1) as usize,
            frame_carry: VecDeque::new(),
        }
    }

    fn push(&mut self, samples: &[f32], source_rate: u32) -> Result<Vec<Vec<f32>>, String> {
        if source_rate == 0 {
            return Err("Realtime 音訊 sample rate 不可為 0".to_string());
        }
        match self.source_rate {
            Some(current) if current != source_rate => {
                return Err("Realtime 音訊 sample rate 在串流期間發生變更".to_string())
            }
            None => self.source_rate = Some(source_rate),
            _ => {}
        }
        self.source.extend(samples.iter().copied());
        self.source_total = self.source_total.saturating_add(samples.len() as u64);
        let output = self.resample_available(false);
        Ok(self.frameize(output, false))
    }

    fn finish(&mut self) -> Vec<Vec<f32>> {
        let output = self.resample_available(true);
        self.frameize(output, true)
    }

    fn resample_available(&mut self, finishing: bool) -> Vec<f32> {
        let Some(source_rate) = self.source_rate else {
            return Vec::new();
        };
        if self.source_total == 0 || self.target_rate == 0 {
            return Vec::new();
        }
        let final_output_len =
            (self.source_total * u64::from(self.target_rate) / u64::from(source_rate)).max(1);
        let mut output = Vec::new();
        loop {
            if self.output_index >= final_output_len {
                break;
            }
            let numerator = self.output_index * u64::from(source_rate);
            let left = numerator / u64::from(self.target_rate);
            let remainder = numerator % u64::from(self.target_rate);
            if left >= self.source_total
                || (!finishing && remainder != 0 && left + 1 >= self.source_total)
            {
                break;
            }
            let offset = (left - self.source_base) as usize;
            let Some(&left_sample) = self.source.get(offset) else {
                break;
            };
            let right_sample = self.source.get(offset + 1).copied().unwrap_or(left_sample);
            let fraction = remainder as f32 / self.target_rate as f32;
            output.push(left_sample * (1.0 - fraction) + right_sample * fraction);
            self.output_index += 1;

            let next_left =
                self.output_index * u64::from(source_rate) / u64::from(self.target_rate);
            while self.source_base < next_left && self.source.len() > 1 {
                self.source.pop_front();
                self.source_base += 1;
            }
        }
        output
    }

    fn frameize(&mut self, output: Vec<f32>, finishing: bool) -> Vec<Vec<f32>> {
        self.frame_carry.extend(output);
        if finishing && !self.frame_carry.is_empty() {
            let remainder = self.frame_carry.len() % self.frame_samples;
            if remainder != 0 {
                self.frame_carry
                    .extend(std::iter::repeat_n(0.0, self.frame_samples - remainder));
            }
        }
        let mut frames = Vec::new();
        while self.frame_carry.len() >= self.frame_samples {
            frames.push(self.frame_carry.drain(..self.frame_samples).collect());
        }
        frames
    }
}

struct DrainedAudio {
    chunks: Vec<LiveAudioChunk>,
    producer_closed: bool,
}

fn drain_ready_audio(audio: &Receiver<LiveAudioChunk>) -> DrainedAudio {
    let mut chunks = Vec::new();
    loop {
        match audio.try_recv() {
            Ok(chunk) => chunks.push(chunk),
            Err(TryRecvError::Empty) => {
                return DrainedAudio {
                    chunks,
                    producer_closed: false,
                }
            }
            Err(TryRecvError::Disconnected) => {
                return DrainedAudio {
                    chunks,
                    producer_closed: true,
                }
            }
        }
    }
}

async fn send_stream_frame(
    session: &mut RealtimeSession,
    vad: &mut ClientVad,
    local_vad: bool,
    frame: Vec<f32>,
    frame_samples: usize,
) -> Result<(), String> {
    if local_vad {
        for event in vad.push(&frame) {
            match event {
                VadEvent::Started { audio } | VadEvent::Continued(audio) => {
                    send_f32_chunked(session, audio, frame_samples).await?;
                }
                VadEvent::Ended(_) => session
                    .finalize()
                    .await
                    .map_err(|error| error.to_string())?,
            }
        }
    } else {
        send_f32_chunked(session, frame, frame_samples).await?;
    }
    Ok(())
}

async fn send_f32_chunked(
    session: &RealtimeSession,
    samples: Vec<f32>,
    frame_samples: usize,
) -> Result<(), String> {
    let pcm: Vec<i16> = samples
        .into_iter()
        .map(|sample| (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
        .collect();
    for frame in pcm.chunks(frame_samples) {
        session
            .send_audio(frame.to_vec())
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn frames(duration_ms: u64, frame_ms: u64) -> usize {
    duration_ms.div_ceil(frame_ms).max(1) as usize
}

async fn wait_for_cancellation(cancelled: &AtomicBool) {
    while !cancelled.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vad_duration_conversion_is_bounded_to_at_least_one_frame() {
        assert_eq!(frames(0, 10), 1);
        assert_eq!(frames(25, 10), 3);
    }

    #[test]
    fn finalize_is_emitted_only_after_all_audio_and_producer_disconnect() {
        let (sender, receiver) = mpsc::sync_channel(4);
        sender
            .send(LiveAudioChunk::from_samples(vec![1.0], 16_000))
            .expect("audio 1");
        sender
            .send(LiveAudioChunk::from_samples(vec![2.0], 16_000))
            .expect("audio 2");
        drop(sender);

        let drained = drain_ready_audio(&receiver);
        let mut wire = drained
            .chunks
            .iter()
            .map(|chunk| format!("audio:{}", chunk.samples[0]))
            .collect::<Vec<_>>();
        if drained.producer_closed {
            wire.push("finalize".to_string());
        }
        assert_eq!(wire, ["audio:1", "audio:2", "finalize"]);
    }

    #[test]
    fn streaming_44100_resampler_preserves_phase_and_fixed_10ms_frames() {
        let input = (0..4_417)
            .map(|index| ((index as f32) * 0.013).sin())
            .collect::<Vec<_>>();
        let mut converter = StreamingAudioConverter::new(16_000);
        let mut frames = Vec::new();
        for chunk in input.chunks(137) {
            frames.extend(converter.push(chunk, 44_100).expect("streaming resample"));
        }
        frames.extend(converter.finish());
        assert!(frames.iter().all(|frame| frame.len() == 160));
        let actual = frames.into_iter().flatten().collect::<Vec<_>>();
        let mut expected = RecordedAudio {
            samples: input,
            sample_rate: 44_100,
            channels: 1,
        }
        .mono_resampled(16_000);
        expected.resize(expected.len().div_ceil(160) * 160, 0.0);
        assert_eq!(actual.len(), expected.len());
        for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert!(
                (actual - expected).abs() < 1.0e-5,
                "sample {index}: actual={actual} expected={expected}"
            );
        }
    }

    #[test]
    fn cancelled_worker_is_joinable_within_bound() {
        let (control, _control_rx) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = Arc::clone(&cancelled);
        let join = std::thread::spawn(move || {
            while !worker_cancelled.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
        });
        let mut handle = RealtimeWorkerHandle::from_parts_for_test(control, cancelled, join);
        handle.cancel();
        let deadline = std::time::Instant::now() + Duration::from_millis(250);
        while !handle.is_finished() && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(handle.join_if_finished());
    }
}
