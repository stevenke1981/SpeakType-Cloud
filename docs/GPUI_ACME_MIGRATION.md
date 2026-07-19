# SpeakType Cloud — GPUI + AcmeUIKit Migration

## Goal

Replace the current `eframe/egui` presentation layer with a native GPUI frontend built from AcmeUIKit, while preserving the proven audio, hotkey, provider, realtime, secret storage, updater, history, injection, and Windows integration code.

## Migration strategy

The migration uses two frontends during development:

- `speaktype-cloud`: existing stable eframe/egui application.
- `speaktype-cloud-acme`: new GPUI + AcmeUIKit application behind the `gpui-ui` feature.

This prevents the production path from breaking while the GPUI shell reaches feature parity.

## Commands

```powershell
# Existing stable frontend
cargo run --bin speaktype-cloud

# New GPUI + AcmeUIKit frontend
cargo run --no-default-features --features gpui-ui --bin speaktype-cloud-acme
```

## Target architecture

```text
src/
├── core/                    # UI-independent domain and orchestration
│   ├── commands.rs
│   ├── events.rs
│   ├── state.rs
│   └── runtime.rs
├── audio.rs
├── config.rs
├── history.rs
├── hotkey.rs
├── injector.rs
├── providers/
├── realtime/
├── secrets.rs
├── startup.rs
├── transcription.rs
├── updater.rs
├── vad.rs
├── ui_egui/                 # temporary compatibility frontend
├── ui_gpui/                 # final AcmeUIKit frontend
│   ├── app.rs
│   ├── dashboard.rs
│   ├── settings.rs
│   ├── api_keys.rs
│   ├── history.rs
│   ├── update.rs
│   ├── recorder.rs
│   └── tray.rs
└── bin/
    └── speaktype_cloud_acme.rs
```

## AcmeUIKit component mapping

| SpeakType feature | AcmeUIKit components |
|---|---|
| Main shell | `TitleBar`, `NavigationRail`, `Card`, `StatusBar`, `Badge` |
| Recording status | `Progress`, `Spinner`, `Alert`, `Badge`, `Button` |
| Provider/model selection | `Select`, `Combobox`, `SegmentedControl`, `RadioGroup` |
| API key management | `PasswordInput`, `Form`, `FormMessage`, `Dialog` |
| Settings | `SettingsPage`, `SettingsGroup`, `SettingsRow`, `Switch`, `NumberInput` |
| History | `DataGrid`, `SearchInput`, `Pagination`, `EmptyState`, `ContextToolbar` |
| Updates | `Dialog`, `Progress`, `Alert`, `Button` |
| Confirmation flows | `Dialog`, `ModalBackdrop`, `FocusTrap` |
| Global shortcuts | `Kbd`, `ShortcutManager` |
| System integration | `SystemTray`, `WindowControls`, `AboutDialog` |

## Milestones

### M0 — Foundation

- Add optional AcmeUIKit and GPUI dependencies.
- Add a separate GPUI binary.
- Open a themed AcmeUIKit window.
- Keep the existing binary unchanged and runnable.

### M1 — Shared application state

- Extract `SpeakTypeCloudApp` behavior from egui-specific rendering.
- Define UI-independent `AppCommand`, `AppEvent`, and `AppSnapshot` types.
- Move background worker communication to shared channels.
- Add deterministic state transition tests.

### M2 — Main recording workflow

- Connect global hotkey state to GPUI.
- Display idle, recording, uploading, transcribing, success, and error states.
- Connect provider/model selection.
- Preserve clipboard restore and focused-window injection behavior.

### M3 — Settings and secrets

- Port all settings controls to AcmeUIKit.
- Port Windows Credential Manager API key storage.
- Add validation and confirmation dialogs.
- Preserve import-only handling for legacy environment keys.

### M4 — History and updates

- Port transcription history and audio playback.
- Add searchable DataGrid history.
- Port secure update checks, staging, verification, and launch UI.

### M5 — Desktop integration

- Rebuild tray show/hide/exit behavior for GPUI.
- Validate login startup controls.
- Restore window position and taskbar behavior.
- Add Windows 10 and Windows 11 lifecycle tests.

### M6 — Cutover

- Complete parity checklist.
- Make GPUI the default binary.
- Retain egui behind a temporary compatibility feature for one release.
- Remove egui only after release smoke tests pass.

## Feature-parity gate

The GPUI frontend must not replace the current frontend until all items pass:

- Batch PTT recording and transcription.
- OpenAI, xAI, and OpenRouter batch providers.
- OpenAI and xAI realtime modes.
- Continuous dictation and VAD.
- Global hotkey press/release behavior.
- Clipboard restore and focused-window injection.
- API key save, clear, mask, and legacy import.
- Settings persistence and validation.
- History save, playback, cleanup, and deletion.
- Tray hide/show/exit.
- Startup registration.
- Secure updater behavior.
- Light/dark theme and CJK text rendering.
- Windows 10/11 release build and smoke tests.

## Validation

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo check --no-default-features --features gpui-ui --bin speaktype-cloud-acme
```

The GPUI build requires the same pinned Zed/GPUI revision used by AcmeUIKit. AcmeUIKit is pinned to a commit in `Cargo.toml` so API changes do not silently break SpeakType Cloud.
