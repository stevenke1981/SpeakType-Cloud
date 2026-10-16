# Checkpoint 05 — independent review fixes and native lifecycle evidence

Base: `bf5464a09c1bef1f050880abb511a754bbbfada8`. Review the immutable commit containing this document. Version remains unpublished 0.2.0. No merge, tag or Release. The independent checkpoint04 review requested changes; this checkpoint requires fresh review and matching CI.

## Review fixes

- CP4-F1 (`src/app.rs`): terminal Realtime cleanup recomputes busy only after releasing the receiver and finished worker. Realtime ownership exposes busy/Cancel while capture has stopped. Cancel handles receiver-only ownership too. Tests inject an error, exercise Stopped/disconnect, prove no premature restart, and prove subsequent restart. Batch ownership regression stays covered.
- CP4-F2 (`src/postprocess.rs`, `src/app.rs`): structured cleanup uses a formatting-preserving final path. CRLF/CR normalize to LF; forbidden controls are removed; list/paragraph line breaks survive dictionary/conversion and accepted-result processing. The integration test asserts editable preview and the actual injected HistoryStore payload, with Google/clipboard/injection disabled. Ordinary raw-transcript behavior is retained.
- CP4-F3 (`src/startup.rs`): explicit settings Save reconciles the app-owned startup value even if the boolean is unchanged. Missing/old commands become the quoted current executable; off removes this app's value. Existing config rollback tests remain. Installer notes now explain Save even when already checked. Only an injected store is used in these tests, not the user's Run entries.
- Advanced baseline parity (`src/slint_settings.rs`, `ui/settings.slint`): all four key environment-variable names can be edited, validated, saved and reloaded. The UI explicitly says to save names before saving/clearing keys; no existing key is displayed, moved or inspected.

## Lifecycle diagnosis and implementation

`src/slint_ui.rs` centralizes Hide/Show/Exit for normal UI/tray actions. Show calls Slint `ui.show()`, clears minimized state and requests redraw; errors are surfaced. `--demo-lifecycle` is an explicitly offline launch policy, fails if tray creation fails, and uses a distinct visible title. At 10/25/40/55/70 seconds it invokes the real hide callback, shared tray Show handler, Slint close event, shared Show, and shared Exit/drain. It never reads the live profile, opens a microphone, registers live hotkeys, contacts an API or manipulates private apps.

Slint 1.18.1 source (`i-slint-backend-winit/winitwindowadapter.rs`) skips drawing while its own state is Hidden. External native ShowWindow does not update that state. This explains why the previous externally restored QA window cannot establish a tray defect. The new test validates Slint's actual lifecycle; it does **not** synthesize or claim an actual tray mouse click.

## Automated evidence

All local paths below are under `E:\Codex\2026-10-17\task`.

- `cp5-review-red.log`: all three requested issues fail; `cp5-realtime-latch-red.log`: separate terminal busy-latch reproduction fails. `cp5-review-green.log`: four regressions pass.
- `cp5-env-red.log`: missing environment-name editing fields fail compilation before implementation. The final gate passes this added test.
- `lifecycle-red.log` / `lifecycle-green.log`: diagnostic launch policy fails before implementation, then passes.
- `checkpoint5-check-approved.log`: complete `scripts/check.ps1 -TestRoot <workspace>` exit 0; fmt, all-feature clippy, **162 default tests**, **175 legacy comparison tests**, **24 packaging behavior cases**, release build and PE import gate. SHA256 `a5715df831c250a8d70766dba440111a48a11533d7ed56e2326f0569e43888d0`.
- The initial sandbox run failed 28 localhost mock requests with OS 10013; the approved local-loopback run passed. This was not an external API test. Initial default-temp packaging move also encountered access denial; rerun used the explicit writable workspace test root.
- Candidate EXE SHA256: `46132fd4e84773566bcc1f70e8c62824cfaf9a3b613696430b485496269ec70b`.
- Local unsigned production-name Inno candidate: `dist/installer/SpeakTypeCloud-Setup-0.2.0.exe`, SHA256 `036c6f1ad40e8d623d991979fc71d970e7aab9d5ee0e2b81f30d07df315c9e7f`; portable SHA256 `4df58df8098565f99f21d16a412dd442d1e86e04fee551391d8b3bf916e447f3`; CycloneDX SBOM generated/validated, SHA256 `cd0158d75b0b45fb5776d91605205211d7d1939a7b0f4c9a8de107a2ca133125`. Prior local checkpoint04 installer/portable were preserved under `cp4-preserved-artifacts`; no published asset was replaced. The production-name installer was rebuilt after the explanatory Save wording change; installed fixture used the same EXE and install logic with isolated names.

## Actual Windows 11 / Inno / NSIS evidence

`installer-evidence\qa-f2752d810a\evidence.json` and logs prove:

1. Original repo NSIS install/uninstall logic compiled with only unique identity/path substitutions, using the public v0.1.0 executable. Public portable ZIP digest verified against GitHub: `e383146cd9ed1b0382d3a5f5de59c4dd8e873bca847b8925f85e6452d25db4ea`. Old app was not launched. Official NSIS 3.13 portable digest: `ba63dffc4410ee89193e1cb5a41989991bd77c61068da17e3156d136b7b0b3d8` (download fingerprint, not a signature claim).
2. Actual NSIS install into a custom path with spaces; Inno refused it before file copy (exit 7).
3. Actual normal NSIS uninstaller exited 0; app, its registration and shortcut removed. No synthetic registry stand-in for this transition.
4. Inno then installed the exact current EXE, with Start Menu/Desktop shortcuts targeting `--demo`.
5. Installed EXE ran `--demo-lifecycle`; native screenshots exist at `lifecycle-evidence-20261017-033948\initial.png`, `restored.png`, `close-restored.png`. All three are identical rendered app pixels, SHA256 `e96bef5caa32e9b6ab7dd1dd5509be2c132d2de75183e64cbbd00d7a58666db0`. AutoGui returned no visible match at both hidden observations; empty output did not create a windows.txt file. stdout shows all five stages, stderr is empty, normal exit is 0 at 70.07 seconds (`exit.json`). No external ShowWindow was used in this run.
6. New Inno normal uninstall exited 0; app, both shortcuts and own registration disappeared. All four synthetic profile sentinel hashes stayed unchanged. Actual Credential Manager contents were never read or tested.

Reproduction helpers outside repo: `test-real-nsis-migration.ps1`, `run-lifecycle-evidence.ps1`. The fixture is removed normally; evidence and synthetic profile remain. This is a real install/uninstall transition with isolated names, not an in-place execution of the published installer against a user's existing profile.

## Remaining gates and exact manual handoff

The older checkpoint04 QA process **PID 18836** still targets `installer-evidence\qa-ae2e517b23\Application With Spaces\SpeakTypeCloud.exe`. It was preserved, not killed/uninstalled. One native sequence remains: use its actual tray icon (`SpeakType Cloud · 離線示範`), choose **顯示 SpeakType Cloud**, confirm rendered UI, then tray **退出 SpeakType Cloud**. After exit the author can normal-uninstall this fixture and verify its stored sentinel hashes. This is a current older-candidate cleanup/triage step; the new candidate still needs a real tray-click acceptance sequence too.

NOT_RUN: physical tray click on the new candidate, actual global hotkeys, Windows 10, 100/150/200% DPI/IME matrix, private microphone/real provider, paid Google ASR/cleanup semantics, private-app paste, installer user-cancel/locked-file recovery. No full Typeless parity claim. Fresh independent review and matching final CI are pending. Previous bf5464a CI run 38954715263 was still in progress at this checkpoint's observation.

Correction to prior checkpoint language: the multilingual cleanup corpus contains authored parser fixtures; it never proved a model transformed raw input faithfully. CL04 now has an end-to-end mocked accepted-result formatting test. CL01–CL03/CL05–CL08 real semantic quality remains NOT_RUN; explicit style preferences/suggestions are not hidden learning or training.
