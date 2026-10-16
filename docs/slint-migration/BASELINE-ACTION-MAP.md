# Baseline-to-Slint action gate

At checkpoint 02 this is an implementation map, not a pass report. Every row is
release-blocking until an equivalent Slint action is reachable and tested.
The opt-in offline demo does not satisfy live migration acceptance.

Checkpoint 03 update: all listed destinations now have controller/adapter wiring
in `slint_ui.rs`, `slint_settings.rs`, `slint_services.rs` and `ui/settings.slint`.
The table below retains checkpoint 02 evidence for comparison (its NOT_PORTED
cells are historical, not current implementation status). Current verified
native coverage is limited to offline start/stop/result/retry, equal-result draft
replacement, provider selection, in-memory save and exit; the other actions are
WIRED / NATIVE_NOT_RUN, not accepted parity. Gemini's model picker currently uses
the documented compatible default plus manual model input, not a live catalog.

Checkpoint 05 adds editable/validated credential environment-variable names for
all four providers (names only, never displays values). It also verifies the
installed Slint hide/close → shared tray Show → shared Exit lifecycle with native
screenshots. A physical tray mouse click, live credentials, global hotkey and
private capture/injection remain NOT_RUN. See CHECKPOINT-05.md for precise evidence.

| Baseline surface/action | Existing source | Slint destination | Current evidence |
|---|---|---|---|
| Batch PTT / toggle / cancel / retry | app.rs, transcription.rs | Main dictation | Demo only; live NOT_RUN |
| OpenAI/xAI/OpenRouter and model list | shell.rs settings, providers/models.rs | Provider settings | NOT_PORTED |
| API key save / clear / masked entry | shell.rs key actions, secrets.rs | Credentials panel | NOT_PORTED |
| Microphone, gain, duration, language/prompt | shell.rs/app.rs | Audio settings | NOT_PORTED |
| Realtime / continuous, VAD, Smart Turn | legacy_view.rs, config.rs | Mode/advanced settings | NOT_PORTED; core retained |
| Dictionary / Chinese variant / voice commands | shell.rs, postprocess.rs | Text processing settings | NOT_PORTED |
| Raw/final edit/copy/paste and fallback | app.rs/injector.rs | Main preview | Demo only |
| Clipboard/temporary modes and automatic delivery | config.rs, app.rs | Output settings | NOT_PORTED |
| History list/play/delete/retention | shell.rs, history.rs | History panel | NOT_PORTED |
| Global hotkeys and configuration rollback | hotkey.rs, app.rs | Shortcut settings | NOT_PORTED in Slint |
| Launch at login transaction | startup.rs, shell.rs | General settings | NOT_PORTED |
| Tray show/hide/exit and active-work shutdown | shell.rs | Slint tray/window lifecycle | Implementing demo; native NOT_RUN |
| Signed updater: check/stage/confirm launch | updater.rs, shell.rs | Update panel | NOT_PORTED |
| Single-instance guard | single_instance.rs/main.rs | Slint production bootstrap | NOT_PORTED; demo deliberately separate |

New translation, selected-text editing/asking and cross-site search remain
separate Typeless feature gaps; they are not excuses to waive baseline regressions.
