# Reference Project Analysis

參考倉庫：`stevenke1981/speaktype`

## 保留的設計

- Rust + egui desktop architecture.
- `cpal` microphone capture with f32/i16/u16 handling.
- audio normalization, mono conversion and 16 kHz resampling.
- rdev global hotkey listener.
- clipboard-based Ctrl+V injection for Chinese text.
- background worker and channel-based result delivery.
- manual fallback when auto injection fails.

## 替換的設計

- 移除 `whisper-rs`、GGML model catalog、model downloader、CUDA runtime packaging.
- `Transcriber` 改成 `SpeechToTextProvider` abstraction.
- model-ready state 改成 API credential/config validation.
- local model worker 改成 short-lived HTTP transcription worker.

## 新增的設計

- OpenAI and xAI provider adapters.
- API key environment-variable policy.
- explicit target HWND capture and restoration.
- xAI language-format compatibility guard.
- provider-independent history records.
