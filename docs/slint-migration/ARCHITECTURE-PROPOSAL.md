# Slint migration architecture (proposed)

## Decision and ownership

Retain the Rust audio/network/platform core. Replace egui shell with compiled `.slint` UI and a thin UI adapter. Extract domain lifecycle from `app.rs` without rewriting providers, realtime ordering, VAD, injection or credential internals. A single controller owns state, commands and snapshots; UI stores only editable drafts and projected display state.

Pipeline: UI/hotkey command → controller → recorder/worker → JobId event → controller → Slint projection. Slint callbacks never wait on network, file conversion or worker joins. Poll bounded/completion channels on the UI thread; reject stale generations; cancellation acknowledges worker shutdown before a new job can start. On exit cancel workers, stop capture, stop projections and remove tray. Slint Weak handles prevent cycles.

## Interfaces and migration sequence

1. Establish baseline tests and a `--demo`/isolated data option BEFORE launching the app. Demo must bypass all credential/config/history loading, microphone capture, provider networking, clipboard/injection, startup registry and updater actions. Synthetic audio and explicit mock results are visibly labeled.
2. Extract controller methods: start/stop/cancel/retry/poll/apply-settings/preview/copy/confirm-paste; snapshots expose recording/processing/cancelling/error/preview and operation capability flags. Keep existing testable target tracking and settings rollback.
3. Implement Slint main window with Traditional Chinese labels, microphone/provider/mode/key settings, raw and processed previews, editable final text, explicit copy/paste, history and status/errors. Existing OpenAI/xAI realtime configuration and dictionary remain reachable; functionality not yet ported is listed as a release gap, never silently dropped.
4. Prefer existing `tray-icon` 0.24 integrated with Slint persistent event loop and show/hide/quit events. Choose native Slint SystemTrayIcon only after the exact selected release demonstrably compiles. Keep rdev hotkey registration; tray menu accelerators do not replace global hotkeys.
5. Add Gemini batch provider using one verified official HTTP schema, inline PCM WAV and configurable audio-capable model. Default host fixed to Google's official HTTPS endpoint; no arbitrary remote override leaking keys. Key in header, never URL/log/Debug/config. No Files API upload, background interactions, tool execution or audio-to-action behavior. Request cancellation/timeout bounded; 401/403 non-retryable, 429/5xx bounded retries, empty/blocked/malformed replies actionable and private. Existing providers remain supported.
6. Local deterministic cleanup/dictionary plus transcript/processed preview is initial formatting scope. AI rewrite/translation and selection/search workflows are separately stated gaps unless later explicitly implemented and tested.
7. Separate history text opt-in and audio opt-in, default off for new fields; disabling audio prevents both recordings and history/audio writes. Write history after accepted success; cancellation/failure leaves no orphan recording. Retention/deletion paths operate only on app-owned data, validate IDs and do not touch unrelated files. Opt-out does not silently delete pre-existing data.
8. Inno Setup per-user installer for next SemVer, Start Menu and optional desktop shortcut, runtime dependency inspection, normal uninstall preserves user data/keys. Preserve package checksums, SBOM, provenance, signed update trust and existing assets. CI development branches build candidates only. Publication requires independent review, current CI, real UI and installer evidence, plus explicit merge resolution if needed.
9. Bundle original or license-attributed SVG icons for microphone/record/stop/settings/history/copy. Use a consistent 24-unit viewBox, stroke and theme color; text and tooltips remain available. No remote icon fetch. Tray uses an ICO/RGBA derivative of the same design, not an unsupported SVG claim. Validate real output at 100/150/200% DPI.
10. Review the exact Slint package license before selecting its version. Preserve third-party notices and use a lawful route compatible with distribution. If choosing Slint royalty-free terms, retain required top-level AboutSlint attribution; do not label Slint itself MIT or accept extra agreements on the user's behalf.

Additional provider invariants: disable authenticated cross-origin redirects; never inject an HTTP error or safety refusal as transcript; preserve numbers, names and negation. No Enter key injection or elevation to bypass UIPI. Retain the visible 120-second product capture limit.

## Google disclosure

UI must explicitly say audio/text is sent to Google, user may incur charges, and ask the user to choose/confirm this provider. Do not infer billing or region from a key. Link https://ai.google.dev/gemini-api/terms . Current official terms (effective 2026-03-23, read 2026-10-16) distinguish unpaid-service product improvement/human review, paid-service processing and limited safety logging, and EEA/Switzerland/UK conditions. BYOK is not a zero-retention guarantee. Do not promise parity with Typeless privacy claims.

Only mocks and self-generated audio are authorized this round. Live Google validation requires a separate explicit scope: selected model, synthetic audio duration, estimated/capped cost, request count and user-provided key entered in app. No such request is performed in this checkpoint.

## Review questions

- Can controller extraction retain all realtime lifecycle/target/injection invariants?
- Is demo isolation complete before any GUI startup side effects?
- Are audio/text persistence defaults and cancellation ownership unambiguous?
- Is preview-first paste safe, while preserving opt-in existing automatic delivery?
- Do installer migration and release gates preserve trust and artifact immutability?
