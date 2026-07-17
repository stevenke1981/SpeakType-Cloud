# Source References

Verified on 2026-07-17.

## Reference implementation

- https://github.com/stevenke1981/speaktype
- Inspected default branch: `master`
- Latest inspected commit: `426da35d1261b47bdefe65c6ca71f066a6f144f1`

## OpenAI official documentation

- Audio transcription API: https://developers.openai.com/api/reference/resources/audio/subresources/transcriptions
- Speech-to-text guide: https://developers.openai.com/api/docs/guides/speech-to-text
- Implemented endpoint: `POST /v1/audio/transcriptions`

## xAI official documentation

- Speech to Text: https://docs.x.ai/developers/model-capabilities/audio/speech-to-text
- Voice REST API reference: https://docs.x.ai/developers/rest-api-reference/inference/voice
- Implemented endpoint: `POST /v1/stt`

## Design note

The xAI adapter intentionally appends the multipart `file` field last, following the official API note. The OpenAI adapter uses a configurable model, defaulting to `gpt-4o-mini-transcribe`.
