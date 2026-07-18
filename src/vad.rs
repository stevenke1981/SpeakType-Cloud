use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct VadConfig {
    pub rms_threshold: f32,
    pub pre_roll_frames: usize,
    pub min_speech_frames: usize,
    pub silence_frames: usize,
    pub max_utterance_frames: usize,
}

impl VadConfig {
    #[cfg(test)]
    fn for_test() -> Self {
        Self {
            rms_threshold: 0.05,
            pre_roll_frames: 2,
            min_speech_frames: 2,
            silence_frames: 2,
            max_utterance_frames: 20,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum VadEvent {
    Started { audio: Vec<f32> },
    Continued(Vec<f32>),
    Ended(VadEndReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VadEndReason {
    Silence,
    MaxDuration,
}

pub struct ClientVad {
    config: VadConfig,
    pre_roll: VecDeque<Vec<f32>>,
    candidate: Vec<Vec<f32>>,
    speaking: bool,
    utterance_frames: usize,
    silence_frames: usize,
}

impl ClientVad {
    pub fn new(config: VadConfig) -> Self {
        Self {
            config,
            pre_roll: VecDeque::new(),
            candidate: Vec::new(),
            speaking: false,
            utterance_frames: 0,
            silence_frames: 0,
        }
    }

    #[cfg(test)]
    pub fn is_speaking(&self) -> bool {
        self.speaking
    }

    pub fn push(&mut self, frame: &[f32]) -> Vec<VadEvent> {
        let active = rms(frame) >= self.config.rms_threshold;
        if self.speaking {
            self.utterance_frames += 1;
            if self.utterance_frames >= self.config.max_utterance_frames {
                self.reset_after_endpoint();
                return vec![
                    VadEvent::Continued(frame.to_vec()),
                    VadEvent::Ended(VadEndReason::MaxDuration),
                ];
            }
            if active {
                self.silence_frames = 0;
                return vec![VadEvent::Continued(frame.to_vec())];
            }
            self.silence_frames += 1;
            if self.silence_frames >= self.config.silence_frames {
                self.reset_after_endpoint();
                return vec![VadEvent::Ended(VadEndReason::Silence)];
            }
            return Vec::new();
        }

        if active {
            self.candidate.push(frame.to_vec());
            if self.candidate.len() >= self.config.min_speech_frames {
                let mut audio = Vec::new();
                for buffered in self.pre_roll.drain(..) {
                    audio.extend(buffered);
                }
                for candidate in self.candidate.drain(..) {
                    audio.extend(candidate);
                }
                self.speaking = true;
                self.utterance_frames = self.config.min_speech_frames;
                self.silence_frames = 0;
                return vec![VadEvent::Started { audio }];
            }
            return Vec::new();
        }

        self.candidate.clear();
        self.pre_roll.push_back(frame.to_vec());
        while self.pre_roll.len() > self.config.pre_roll_frames {
            self.pre_roll.pop_front();
        }
        Vec::new()
    }

    fn reset_after_endpoint(&mut self) {
        self.speaking = false;
        self.utterance_frames = 0;
        self.silence_frames = 0;
        self.candidate.clear();
        self.pre_roll.clear();
    }
}

fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    (frame.iter().map(|sample| sample * sample).sum::<f32>() / frame.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(level: f32) -> Vec<f32> {
        vec![level; 160]
    }

    #[test]
    fn noise_and_short_bursts_do_not_create_utterances() {
        let mut vad = ClientVad::new(VadConfig::for_test());
        for _ in 0..10 {
            assert!(vad.push(&frame(0.005)).is_empty());
        }
        assert!(vad.push(&frame(0.2)).is_empty());
        assert!(vad.push(&frame(0.005)).is_empty());
        assert!(!vad.is_speaking());
    }

    #[test]
    fn speech_includes_preroll_and_ends_after_silence() {
        let mut vad = ClientVad::new(VadConfig::for_test());
        vad.push(&frame(0.01));
        vad.push(&frame(0.01));
        assert!(vad.push(&frame(0.2)).is_empty());
        let started = vad.push(&frame(0.2));
        assert!(matches!(started.as_slice(), [VadEvent::Started { .. }]));
        assert!(vad.is_speaking());
        assert!(vad.push(&frame(0.0)).is_empty());
        let ended = vad.push(&frame(0.0));
        assert_eq!(ended, vec![VadEvent::Ended(VadEndReason::Silence)]);
    }

    #[test]
    fn maximum_utterance_is_a_deterministic_endpoint() {
        let mut config = VadConfig::for_test();
        config.max_utterance_frames = 3;
        let mut vad = ClientVad::new(config);
        vad.push(&frame(0.2));
        vad.push(&frame(0.2));
        let ended = vad.push(&frame(0.2));
        assert!(ended.contains(&VadEvent::Ended(VadEndReason::MaxDuration)));
    }
}
