# API Provider Notes

## OpenAI

- Endpoint: `POST {base_url}/v1/audio/transcriptions`
- Required multipart: `file`, `model`
- Optional: `language`, `prompt`, `response_format=json`
- Default model in this kit: `gpt-4o-mini-transcribe`
- API key environment variable: `OPENAI_API_KEY`

## xAI

- Endpoint: `POST {base_url}/v1/stt`
- Multipart file must be appended after other form fields.
- Optional: `format`, `language`, repeated `keyterm`.
- API key environment variable: `XAI_API_KEY`
- xAI 的 formatting language 清單目前不包含 `zh`。程式在語言為 `zh` 時省略 `language` 與 `format=true`，但仍上傳音訊進行辨識。

## 供應商切換

設定畫面改變 `provider` 後儲存。每次辨識工作建立對應 provider；不共用 API Key，也不把 key 傳入 UI log。
