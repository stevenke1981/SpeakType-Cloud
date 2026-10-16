# API Provider Notes

## OpenAI

- Endpoint: `POST {base_url}/v1/audio/transcriptions`
- Required multipart: `file`, `model`
- Optional: `language`, `prompt`, `response_format=json`
- Default model in this kit: `gpt-4o-mini-transcribe`
- API key environment variable: `OPENAI_API_KEY`
- Realtime endpoint: `wss://api.openai.com/v1/realtime`
- Realtime model: `gpt-realtime-whisper`; input is 24 kHz mono PCM16 encoded in `input_audio_buffer.append`.
- `transcription.delay` accepts only `minimal`, `low`, `medium`, `high`, or `xhigh`. OpenAI realtime uses local VAD and manual commit.

## xAI

- Endpoint: `POST {base_url}/v1/stt`
- Multipart file must be appended after other form fields.
- Optional: `format`, `language`, repeated `keyterm`.
- API key environment variable: `XAI_API_KEY`
- xAI 的 formatting language 清單目前不包含 `zh`。程式在語言為 `zh` 時省略 `language` 與 `format=true`，但仍上傳音訊進行辨識。
- Realtime endpoint: `wss://api.x.ai/v1/stt`; input is raw 16 kHz mono PCM16 binary frames.
- `smart_turn=<threshold>` and `smart_turn_timeout=<1..5000>` are opt-in. Only `speech_final=true` closes an utterance; chunk-final remains partial.

## OpenRouter

- Endpoint: `POST {base_url}/v1/audio/transcriptions`
- 使用 JSON body：`{"model":"<model>","input_audio":{"data":"<base64 WAV>","format":"wav"},"language":"<ISO>","prompt":"<hint>"}`（非 multipart）。
- Header：`Authorization: Bearer <key>`、`Content-Type: application/json`。
- 預設模型：`openai/gpt-4o-mini-transcribe`（可在設定中更換）。
- 預設 base_url：`https://openrouter.ai/api`。
- API key 環境變數：`OPENROUTER_API_KEY`。
- OpenRouter **不支援 Realtime 或 Continuous Dictation**。選擇 OpenRouter 作為 provider 時，辨識模式必須為 Batch / PTT；若嘗試使用 Realtime 模式，設定驗證會直接拒絕。

## 供應商切換

設定畫面改變 `provider` 後儲存。每次辨識工作建立對應 provider；不共用 API Key，也不把 key 傳入 UI log。切換 provider 或 realtime 設定會先停止目前 session；batch fallback 必須由使用者明確確認。

## Google AI Studio / Gemini (0.2.0)

- Batch: `POST https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent`; sensitive `x-goog-api-key` header, never a URL key.
- Default `gemini-3.8-flash`; manual model names are bounded and validated. Account/model availability requires user verification.
- Inline base64 WAV up to 8 MiB; no Files API, cached history or tools. Auto language omits an explicit language hint; preserve original and mixed languages.
- Explicit Google data-transfer consent required. Optional cleanup sends another text request and may incur another charge. No live Google request was used in offline acceptance.
- Redirects disabled; bounded response and timeout. STOP plus nonempty visible text required; blocked/truncated/malformed/empty results are errors. Batch transient errors have bounded retry/cancellation.
- Gemini Realtime/Continuous is unavailable; no claim to implement Live API.
- Official references: https://ai.google.dev/api/generate-content , https://ai.google.dev/gemini-api/docs/models/gemini-3.8-flash , https://ai.google.dev/gemini-api/terms . BYOK does not imply zero retention.
