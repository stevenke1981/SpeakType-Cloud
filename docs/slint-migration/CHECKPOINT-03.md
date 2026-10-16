# Checkpoint 03 — implementation and native demo evidence

Base: `803f277e23e72d4b2fe97352ea26708371cc1675`. The immutable checkpoint is the commit containing this document; its SHA is reported in the handoff. It is not release acceptance.

## Review corrections

- CP2-F1: Batch admission now checks the owned job and receiver independently of presentation `busy`. `fail()` preserves busy while any worker is owned. An injected settings-write failure holds the old channel open, rejects recording/retry/replacement, and admits a new recording only after disconnect. RED then GREEN: `review-f1-red.log`, `review-f1-green.log` in the parent workspace.
- CP2-F2: preview projection uses an accepted-result revision. Two equal completions reset an intervening unsent edit; timer ticks preserve that edit. RED then GREEN: `review-f2-red.log`, `review-f2-green.log`. This is a reducer/controller regression, not a claim of native reproduction on the new candidate.

## Provider and data boundary

- Gemini uses `POST https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent`, pinned default `gemini-3.8-flash`, `x-goog-api-key` sensitive header, inline WAV. No Files upload, cached content, tools, Interactions history or background request is used. Redirects are disabled; model names are bounded; responses are bounded to 1 MiB and error bodies are never echoed.
- Only a complete STOP result with nonempty non-thought text is accepted. HTTP 408/429/5xx and transport timeouts retain the existing bounded retry/cancel contract. Blocked, malformed, empty and truncated results do not become transcripts.
- Google transmission requires explicit app consent. API key input is masked and cleared after submission, with Windows Credential Manager retaining the same existing namespace. The demo path never reads or writes credentials.
- Official references checked 2026-10-16: [GenerateContent](https://ai.google.dev/api/generate-content), [Gemini 3.8 Flash](https://ai.google.dev/gemini-api/docs/models/gemini-3.8-flash), [Google terms](https://ai.google.dev/gemini-api/terms). Free and paid data rules differ; no blanket no-training/zero-retention claim.

## Privacy and cleanup

- Removed both pre-success WAV writes. Text and audio retention are independently opt-in, default false. An accepted result uses immutable options and an injected storage root; cancellation cannot enter persistence. Storage runs off the UI/audio threads, serializes concurrent changes, bounds/parses the JSONL strictly, atomically replaces its manifest and rolls back newly written audio if manifest commit fails. Malformed existing history is preserved and reported.
- History IDs and audio basenames are validated before opening/deleting. The actual store is exercised using generated workspace fixtures, including four text/audio combinations, concurrent commits, retention, traversal rejection and corrupt-file preservation. Temp-directory replacement was denied by the managed sandbox; the same tests passed using owned workspace fixtures.
- Eight requested controls are represented in settings and the Gemini cleanup contract: contextual fillers, accidental repetitions, explicit corrections, paragraphs/lists, conservative polish, tone/style preferences, explicit dictionary/style suggestion approval, and original-language/mixed-language preservation. Cleanup is an additional optional request and always requires preview confirmation; no automatic paste follows cleanup. Original text can be restored.
- Contract corpus includes synthetic zh-TW, English, Japanese and mixed examples. These are parser/prompt fixtures, **not** measured model quality or live ASR coverage. No Google requests were made.

## Slint candidate scope

The candidate selects Slint by default. A separate legacy feature remains for comparison tests. Slint panels expose providers/models, credentials, microphones, shortcuts, launch at login, output modes, both retention switches, dictionary/voice commands, advanced realtime/VAD/Smart Turn, history playback/deletion/retention, original restore, suggestion approval and signed updater check/download/confirmed launch. Native parity and individual operation acceptance remain pending. Configured signing is still required by the existing updater; there is no unsigned bypass.

## Native evidence for the previous frozen demo

After the user said the desktop was ready, AutoGui reported `ready=true`, `cursor_access=true`, successful capture, remote session 1 / console session 3. No session/security settings were changed.

The standalone offline artifact from `803f277` (SHA-256 `ed5ca9bec0070a7e2fc3c07e478345c6f0f84baa37a4e585d3f9bea5fc0d188b`) was launched with `--demo`. Normal sandbox input did not change the UI and window moves were denied. Approved execution outside the tool sandbox, under the same normal user account, successfully moved/resized the owned demo and performed Start → Stop → result preview → Exit. No administrator elevation, microphone, private app input, key access or API was used. App exit code was 0.

Local screenshots under `E:\Codex\2026-10-17\task`: `slint-demo-start-confirmed.png`, `slint-demo-result.png` (processing), `slint-demo-completed.png` (complete). Complete screenshot SHA-256: `da82bf5df1a61fb827a4925a6efd8496fe1ed3fd4d9e0f17cfb1eaeb341966f1`. These are local review evidence only; they are not added as public repository assets. The new candidate UI, native tray menu restore, IME/DPI, live microphone, Google quality and install/upgrade/uninstall remain NOT_RUN until individually verified.

## Prior CI

[Run 38945770124](https://github.com/stevenke1981/SpeakType-Cloud/actions/runs/38945770124) completed **success** for exact SHA `803f277`. This does not establish CI for the working candidate.

## Remaining release gates

Candidate validation: `scripts/check.ps1 -TestRoot E:\Codex\2026-10-17\task` exited 0. Release static tests, 24 packaging behavior cases, format, all-feature clippy, **152 Slint/core tests**, **166 legacy comparison tests**, and optimized default Slint build passed. Log `E:\Codex\2026-10-17\task\checkpoint3-check.log`, SHA-256 `365f956af62035025f3c75d4b64ae7545070871f986f993384bf411a3311d621`.

Candidate EXE `dist/preview/SpeakTypeCloud-checkpoint3.exe`, SHA-256 `20d8cfcaf6d82660dc4aef7693ffaba23e6c67c876a7d64bf826b5d591d537d1`. Started with `--demo` using a waited process; native Start → Stop → original/final preview → retry → settings/provider dropdown → Gemini → save in memory → Exit passed; process exit 0.

Native CP2-F2 regression: focused the actual editable preview, inserted synthetic `未送出草稿`, then retried the identical mock. Screenshot confirms the unsent edit was replaced by the new result. Evidence: `cp3-unsent-draft.png`, `cp3-repeat-result.png`. This is now native evidence in addition to the reducer test.

Candidate native screenshots under the same parent workspace: `cp3-initial.png`, `cp3-recording.png`, `cp3-completed.png`, `cp3-settings.png`, `cp3-gemini-settings.png`, `cp3-settings-saved.png`. All are isolated demo screenshots; no private settings or live provider was used. Other panel actions, tray menu, IME, DPI variants and active-worker shutdown still require acceptance.

Verified official Inno Setup 7.1.0 x64 compiler download SHA-256 `0362a383ed217d4c4239b5933866dd96d3eb2102737da92f80f6057a4b40df2f`, Authenticode **Valid**, publisher **Pyrsys B.V.**. Installed current-user into the task tools folder, no icons or reboot. App installer implementation is the next slice.

Independent immutable-SHA review; candidate full CI; actual Slint interaction coverage; Inno Setup implementation and old-NSIS migration fixtures; real installation/start/uninstall with data preservation; complete baseline action acceptance. No tag, merge or Release has been created.
