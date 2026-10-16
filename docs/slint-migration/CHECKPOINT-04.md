# Checkpoint 04 — Inno packaging and lifecycle hardening

Base: `de74fe42b1d8a96043ca00f4e9228e10e06a663a`. Review the immutable commit containing this document. Version is now 0.2.0. This checkpoint is **not release approval**; no merge, tag or Release has been created.

## Changes and review entry points

- `src/app.rs`, `src/slint_ui.rs`, `src/slint_services.rs`: closing rejects new recording/retry, cancels owned speech work, drains receivers and persistence asynchronously, then exits. No blocking join in the Slint callback. Suggestions are projected only when changed, preserving focused controls. Main action tooltips are bundled Slint tooltips.
- `src/config.rs`, `src/transcription.rs`, `src/realtime/openai.rs`: auto-language defaults and omission of the API language hint; existing explicit language remains. Settings use same-directory atomic replacement. An exclusive Windows file fixture verifies a failed replacement leaves the previous bytes intact and retry succeeds.
- `src/history/store.rs`: deletion preflights a manifest write, keeps rows visible until requested audio deletion succeeds, then commits the filtered manifest. A locked audio file leaves a visible retryable row. A failed final manifest commit may leave rows referencing already-deleted requested audio; retry tolerates missing audio. It does not hide a locked undeleted audio file.
- `.cargo/config.toml`, `scripts/check-runtime.ps1`: static MSVC CRT; normal and delayed PE import verification rejects non-system DLLs.
- `installer/SpeakTypeCloud.iss`, `scripts/build-installer.ps1`, `scripts/install-inno-build-tool.ps1`: official hash/signature-pinned Inno 7.1.0, current-user install, separate stable AppId, Start Menu and optional desktop shortcut, app mutex guard, bundled notices/Slint license. Old NSIS registration in either registry view or remembered custom-path remnants block installation. The user normally uninstalls old NSIS first; no registry command execution, forced app termination or recursive data deletion. Old NSIS source remains for reference.
- `scripts/test-inno-install.ps1`: real Inno execution with unique QA branding, synthetic legacy registration/remnants, custom directory containing spaces, installed hash and shortcut verification, repair and normal uninstall. It does not claim that synthetic legacy markers are a real NSIS-uninstaller migration or that a fake credential file tests Credential Manager.
- CI now builds Inno; portable includes third-party notices and Slint terms. Existing signing/manifest trust is preserved. Documentation/config examples reflect preview-first output and separate opt-in retention.

## Eight requested feature acceptance states

| ID | Implemented behavior / reachable control | Evidence and remaining scope |
|---|---|---|
| CL01 | Contextual filler removal toggle | Structured prompt/parser corpus PASS; live semantic fidelity NOT_RUN |
| CL02 | Accidental repetition removal toggle, emphasis preserved by contract | Offline contract PASS; live distinction NOT_RUN |
| CL03 | Explicit spoken self-correction toggle, raw text retained | Offline number/negation/correction cases PASS; live quality NOT_RUN |
| CL04 | Clear-intent paragraphs/lists/steps toggle | Structured contract PASS; live formatting quality NOT_RUN |
| CL05 | Optional conservative wording improvement, editable preview and original restore | Controller/UI wiring PASS; no live model quality claim |
| CL06 | Explicit tone/custom style; style suggestion requires accept then save | Settings round-trip and guarded suggestion parsing PASS; no hidden training or learned profile from accepted edits |
| CL07 | Manual dictionary/commands plus term suggestion accept/reject | Baseline deterministic tests and settings round-trip PASS; all native editing actions not yet individually accepted |
| CL08 | Auto plus explicit language, preserve mixed language | Auto hint omission test PASS; zh-TW/en/ja/mixed synthetic corpus; real ASR/multilingual quality NOT_RUN |

All eight are controls/contracts, not proof of model quality. Cleanup requires opt-in Google transmission and an additional request, preserves the raw transcript on error, and suppresses automatic paste. No paid API, real key, private microphone or private-app typing was used.

## Automated validation

`scripts/check.ps1` exited 0: 24 release behavior tests, fmt, all-feature clippy, **156 default Slint/core tests**, **170 legacy comparison tests**, optimized release build. Log: `E:\Codex\2026-10-17\task\checkpoint4-check.log`, SHA-256 `eae7d188d388b560c6c50a19e2ac4f6e1f4f88bf751e027418fa11bd882a6f97`.

New meaningful RED/GREEN evidence under the parent workspace: `shutdown-red.log` / `shutdown-green.log`; `history-delete-red.log` demonstrates disappearance of the locked audio row before the fix; `settings-save-red.log` before atomic helper implementation; all final tests pass in `cp4-tests.log` and the complete gate. `runtime-red.log` rejects the preceding VCRUNTIME140.dll dependency; `runtime-green.json` confirms current imports are Windows system DLLs only. Example TOML was independently parsed and privacy defaults verified. Static release tests were rerun after packaging script changes.

Candidate EXE SHA-256: `1a9e668b5aa224cc3a82447683405af7c0d01c54f4ed4fb9a836ee9aac0f059b`.

Local unsigned installer: `dist/installer/SpeakTypeCloud-Setup-0.2.0.exe`, SHA-256 `af07d4aacbdb6a78fca535c0fa6e899ab824c830b6a79537580c9f796ee63b44`. Portable SHA-256 `209b45133412d58334af23bc6cbfaf03dfb7fe1a6ae72c87e6c6b47d1b6f2f80`. These are review artifacts, not published assets. CycloneDX SBOM generated and schema/reference checks passed.

Checkpoint 03 CI is confirmed successful for both runs [38950915598](https://github.com/stevenke1981/SpeakType-Cloud/actions/runs/38950915598) and [38950808452](https://github.com/stevenke1981/SpeakType-Cloud/actions/runs/38950808452). This does not substitute for this checkpoint's matching CI.

## Actual Windows evidence

Windows 11 5900X, active remote session 1, AutoGui readiness true and cursor access true. No session or security settings changed.

1. `E:\Codex\2026-10-17\task\installer-evidence\qa-29f12b34bf\`: real Inno fresh install and repair of frozen checkpoint 03 EXE; legacy registration and remembered custom-path remnants correctly rejected with exit 7. Installed hash/licenses/current-user shortcuts verified. Actual installed `--demo`: Start → Stop → original/processed preview → history → delete confirmation → empty history → Exit **0**. Normal Inno uninstall **0**, executable/shortcuts/uninstall registration removed, all four synthetic profile hashes unchanged. See `evidence.json`, native PNGs and install/uninstall logs.
2. `...\installer-evidence\qa-ae2e517b23\`: same real fresh/repair/refusal checks using the new 0.2.0 static-CRT EXE. Actual installed startup and initial Chinese UI captured as `native-initial.png`.
3. Tray acceptance is **not passed**. The QA helper posted WM_CLOSE and then called direct Win32 ShowWindow through AutoGui, bypassing Slint's own `ui.show()` path. This yielded a blank externally-restored window; a later visible-list assertion failed. `native-close.json`, `native-restored.png` retain the evidence. Do not call this actual tray restore or a proven product defect without testing the supported tray action.

The second isolated demo (PID 18836 at observation, exact path under `qa-ae2e517b23\Application With Spaces`) remains running; it has not been force-terminated. Actual tray tooltip is **SpeakType Cloud · 離線示範**, menu **顯示 SpeakType Cloud / 退出 SpeakType Cloud**. It needs a real tray Show/Exit action, then normal fixture uninstall and sentinel verification. Revalidate the process path/PID before any cleanup.

AutoGui's available interface has window-bound image clicks but no direct tray selection/right-click action. The Computer Use skill was read, including guidance/API/confirmation files; its required `node_repl` / `@oai/sky` entry point is not exposed in this task. Native APIs in the separate browser CUA tool are disabled. No custom Computer Use helper protocol, security change or privileged UI workaround was used. This is a pending tool-capability blocker, not an approval denial.

## Remaining release gates

Independent immutable-SHA review; matching full CI; actual tray show/exit and remaining native action coverage; normal uninstall of the second fixture after native recovery; real legacy NSIS removal/upgrade; cancellation/locked-installer rollback cases; Windows 10, physical hotkey/microphone, IME and DPI variants. Live Google semantic/ASR evaluation requires separate minimal approval for explicitly selected synthetic audio/text and possible charges. Do not present mock contracts as those results or claim full Typeless parity.
