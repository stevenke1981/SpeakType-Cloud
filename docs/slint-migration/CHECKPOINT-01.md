# SpeakType Cloud — checkpoint 01

Task: slint-gemini-20261017. Owner: 5900X implementation agent (sole author).
Status: discovery PASS; architecture PROPOSED; product implementation NOT_RUN.

## Reproducible baseline

- Repository: https://github.com/stevenke1981/SpeakType-Cloud.git (public, MIT).
- Base: `main`, `61ba64ad0f8239ea65b929d29feb72dac330692b`.
- Isolated clone: `E:\Codex\2026-10-17\task\SpeakType-Cloud`.
- Work branch: `feat/slint-gemini-20261017`; clean before these planning documents.
- Existing package/release: 0.1.0 / v0.1.0. Proposed next version: 0.2.0; no tag created.
- Open PR #1: `agent/gpui-acme-foundation` → main. It proposes GPUI, conflicts with the requested Slint direction, and is NOT merged or modified.
- GitHub CI runs 34108345611 and 34107964799 report success on the base SHA. These are historical baseline results, not validation of this branch.
- Latest release v0.1.0 published 2026-09-07. Existing assets must remain untouched.
- Machine name 5900X; registry processor name AMD Ryzen 9 5900X 12-Core Processor; OS reports Windows NT 10.0.26300.0. Windows 10 smoke requires a separate environment.
- rustc 1.99.0, cargo 1.99.0, git 2.55.0.windows.5, gh 2.102.0.
- No running cargo/rustc/git/speaktype-cloud processes observed before starting tests. Scoped E:\Codex checkout inventory found no other SpeakType-Cloud clone; nearby `speaktype` is a different repository (`stevenke1981/speaktype`, master). This is observational evidence, not an OS lock or proof nobody else can write remotely.
- No Inno Setup installation found under Program Files (x86). No secret/config/history contents inspected.

## Instructions actually loaded

Local `C:\Users\steven\.codex\skills\agent-team-workflow`: team-orchestrate SKILL, root AGENTS, chief_orchestrator, organization, delivery workflow, handoff contract/template, team-work SKILL, tech_lead, rust_engineer and concurrency guide.

Local `slint-gui-skills`: root entry, slint-workbench SKILL + guide, slint-architecture SKILL + guide, slint-async-jobs SKILL + guide. Skills do not establish Slint dependency availability; no Slint version is locked yet.

Repository AGENTS, SPEC, ARCHITECTURE, SECURITY and TODOS read. Mandatory final `scripts/check.ps1` comprises release tests, fmt, clippy, unit tests and release build. Repo also requires Windows 10/11 smoke; unavailable platforms will remain NOT_RUN.

## Actual architecture and known defects

Rust 2021 application, egui/eframe 0.27; `main.rs` creates `shell::AppleShell`; shell owns settings, keys, history, tray, updater and embeds `app::SpeakTypeCloudApp`. `app.rs` mixes domain lifecycle and egui painting. Mature modules exist for CPAL capture/resampling/WAV, rdev hotkeys, target-aware clipboard injection, provider adapters, cancellation/retry, realtime WebSockets/VAD, postprocessing/dictionary/voice commands, credentials, startup, history and signed-update validation.

No Slint in the baseline manifest. Installer is NSIS, not Inno Setup. Batch `spawn_batch_transcription` saves history audio before API completion regardless of `save_recordings`; SECURITY explicitly acknowledges this. History settings currently only expose retention days. Correction at checkpoint 02: the initial terminal output appeared corrupted due to console encoding; UTF-8/codepoint inspection confirms the examined source strings are valid Traditional Chinese. No speculative source-encoding rewrite is required.

## Write scope and review entry points

Sole author scope: `src/`, `ui/`, Cargo manifest/lock, build.rs, config examples, README/SECURITY/architecture docs, installer and release scripts/workflow, regression tests. Preserve existing core behavior and old installer assets; no edits to tracked agent runtime databases, existing private user data, sibling checkouts or PR #1.

Reviewer reads this document, `ARCHITECTURE-PROPOSAL.md` and `ACCEPTANCE.md` first; then baseline `src/app.rs`, `src/shell.rs`, `src/config.rs`, `src/transcription.rs`, `src/history.rs`, `src/secrets.rs`, `src/injector.rs`, `.github/workflows/windows.yml`.

Independent architecture review requested before broad UI migration. Final independent review must use immutable SHA and the corresponding CI/artifact hashes. Author cannot approve their own release.

## Evidence at checkpoint

- `gh repo view`, `gh pr list`, `gh run list`, `gh release list`: exit 0 after approved network-capable read. Initial sandbox GitHub queries failed connectivity.
- `git status --short`: clean baseline; `git rev-parse HEAD`: base above.
- `cargo test --locked --offline --all-targets`: exit 1, dependency `crossbeam-queue` absent from cache. No tests executed; dependency retrieval is the next step, not a product test failure.
- `scripts/test-release.ps1`: PASS, installer template and release static validation completed. `scripts/test-release-behavior.ps1`: first 15 checks passed, then FAIL at line 111 `Move-Item` of a generated temporary fixture (`one.exe`), access denied. Outcome is not a complete release-test pass; investigate sandbox/file ACL before retrying. No product installer ran.
- Build, real GUI, audio capture, live providers, installer, current branch CI, review and release: NOT_RUN.
- No API calls, private recording, credential reads, push, PR creation, tag, merge or release performed.

Next owner: parent coordinates independent architecture reviewer; implementation author continues from this isolated branch after checkpoint.
