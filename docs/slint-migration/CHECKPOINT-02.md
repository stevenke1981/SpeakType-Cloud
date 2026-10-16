# Checkpoint 02 — controller boundary and isolated Slint preview

Observed 2026-10-16 18:11 UTC on 5900X. This is a development checkpoint, NOT a
release candidate or completed UI migration. Version remains 0.1.0 pending the
complete 0.2.0 migration. The original `legacy-ui` remains default so existing
workflows are not replaced with placeholders. Build with `--features slint-ui`
and pass `--demo` to enter the Slint preview. Do not launch without `--demo`
during this task: normal startup intentionally retains existing user-profile
and credential behavior, which this task has not authorized exercising.

For safe reviewer execution a separate Slint-only build is available at
`dist/preview/SpeakTypeCloud-demo.exe`; run it with `--demo` only after the
interactive desktop is ready. It was built with
`cargo build --locked --offline --no-default-features --features slint-ui`
(exit 0, 19.74s). Its SHA256 is
`ed5ca9bec0070a7e2fc3c07e478345c6f0f84baa37a4e585d3f9bea5fc0d188b`.
Unlike the default-feature binary, this build rejects normal/no-argument startup
before opening services. Executing it without arguments verified the intended
exit 1 and explanatory message. The demo GUI itself has NOT been launched.
Standalone Slint-only compilation currently emits 70 unused legacy-core warnings;
these are not hidden, and final Slint production lint cleanup remains required.
The combined-feature build and strict clippy complete successfully.

## Implemented

- Exact Slint/slint-build 1.18.1, winit/software renderer/accessibility. Lockfile
  comparison confirms 244 added package versions and zero removed/upgraded
  pre-existing package versions.
- Moved existing egui rendering into `src/legacy_view.rs`; controller lifecycle
  stays in `src/app.rs`. Legacy settings/history/update source remains intact.
- CLI mode parsed before OS/profile services. Demo bypasses config, credential,
  history, microphone enumeration/capture, hotkeys, startup, provider network,
  clipboard and injection. It creates a CPAL host object but never opens a
  capture stream. The only demo system integration is its own window/tray.
- Synthesized tone → real WAV encoder → cancellable mock worker → raw/processed
  editable preview. Copy/paste buttons explicitly simulate actions without
  touching the clipboard or another app. Mock output is prominently labeled.
- Traditional Chinese Slint view; bundled original SVGs; tray Show/Exit wiring
  with persistent event loop. Windows icon is RGBA rendered from the microphone
  SVG. AboutSlint is reachable via top-level 說明 menu and About button. License
  source/hash are in THIRD_PARTY_NOTICES.md. Native render/interaction NOT_RUN.

## Independent review response

Review read from `E:\Codex\2026-10-17\task-2\review-196c055.md` and its handoff.
This author does not self-approve these changes; independent re-review required.

- **R1 addressed in code and contract:** Cancel prevents delivery of an already
  queued success with the same JobId. Realtime partial/final events are ignored
  after cancellation; replacement is blocked while a receiver/worker remains.
  Batch stores accepted settings, realtime stores session settings and checks
  the full settings fingerprint. Disconnects recover. Realtime joins occur only
  after `is_finished`, not merely on Stopped. Deterministic tests cover queued
  success/final, immediate restart, settings invalidation, disconnected workers
  and the real demo worker's cancel/retry path.
- **R2 architecture contract addressed; installer implementation NOT_RUN:**
  stable AppId, legacy custom-path discovery, legacy removal before replacement,
  distinct Inno-owned path, no competing uninstallers, lock/cancel handling,
  preserved data/credential namespace and synthetic upgrade/repair/uninstall
  sentinels are now explicit. Actual native upgrade evidence remains blocking.
- **R3 contract addressed:** BASELINE-ACTION-MAP.md maps every existing workflow
  to its intended Slint surface. Any unported/disabled baseline action blocks
  release. Typeless new-feature gaps are tracked separately.

## Actual verification

- Baseline test executable SHA256:
  `19658cb35afaf5acc33517fa927a5b03f57b2747d72d0233b456ad7dda25017a`.
  Sandbox result: 119 pass / 25 fail, mock sockets denied (Windows 10013).
  Same executable with approved loopback access: **144 pass / 0 fail**, exit 0.
- Fixture probe: moving a generated `.exe` succeeds under task workspace and
  fails under system Temp. No ACL/security changes. Added explicit `TestRoot`
  parameter to release behavior tests and check.ps1; unique temporary subtree
  and existing cleanup remain. Workspace fixtures: **24 pass**, exit 0.
- `cargo test --locked --offline --all-features --all-targets`: **151 pass**,
  exit 0. No provider calls or microphone. All existing tests retained.
- `scripts/check.ps1 -TestRoot E:\Codex\2026-10-17\task`: **PASS, exit 0**.
  Includes release static/24 behavior tests, fmt, strict all-target/all-feature
  clippy, 151 default-feature tests and default-feature optimized build.
  Release build took 1m27s and is still the LEGACY interface, not Slint parity.
- Clippy exceptions are restricted to Slint generated code's `unwrap_used` and
  async-trait's generated `double_must_use`. No hand-written safety test or
  assertion was removed. Default legacy-only build reports unused future Slint
  adapter methods; all-features strict clippy passes.
- Required gate log: `E:\Codex\2026-10-17\task\checkpoint2-check.log`, SHA256
  `dfb3a5d10ec8a7d7a3b106946659cb6bc519d8d2ff66afb4a3d53f2190d60111`.
- Default legacy release executable SHA256:
  `09c4d6b99be09a83959dd891c3c9da53d20f3a177c172f02e1c8f7fb8b1d9b4d`.
  Not installed, launched, uploaded or released.

## Actual blocker and remaining work

Native screen/input readiness check:
`C:\Users\steven\AppData\Local\Programs\AutoGui\autogui-control.exe session status --json`
returned exit 1, `ready:false`, blocker `cursor-unavailable`; cursor access and
primary capture probe failed. Session 1 is remote, console session is 3,
`on_console:false`, `input_desktop:Default`; lock state is unknown. This is a
readiness denial, not a release acceptance result. No attempt was made to unlock,
reconnect, change OS security/session settings, or issue blind input. An active
unlocked interactive desktop is needed before native screenshots, keyboard,
tray, DPI and installer interaction can be verified. These remain BLOCKED or
NOT_RUN, not mock-certified.

The user's 18:11 scope update additionally requires all eight cleanup/style/
dictionary/language behaviors in CLEANUP-REQUIREMENTS.md. They are accepted
implementation work with separate model-quality validation, not deferred gaps.

Gemini adapter, persistence opt-ins, complete Slint settings/history/realtime
surfaces, Inno build/upgrade fixture, final independent review, candidate CI,
Windows 10/11 real acceptance and release remain unfinished. Continue the same
branch; GPUI PR1 remains untouched. No merge/tag/release or paid calls performed.
