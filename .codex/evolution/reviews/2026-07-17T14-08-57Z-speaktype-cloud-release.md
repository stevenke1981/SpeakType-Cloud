# SpeakType Cloud v1 release self-review

## Task summary

Completed the existing Windows Rust MVP to a reproducible v1 automated gate, hardened data-loss and focus-injection failure paths, prepared a portable package, and prepared the source for a public GitHub delivery.

## Corrections and feedback

- The original delivery summary claimed a project skeleton but had not established a trustworthy compile/test baseline.
- The original PowerShell gate could report success after an earlier Cargo failure.
- Independent review found stale package staging, focus races, malformed-config overwrite risk, ignored history/clipboard errors, self-window injection, and invisible hotkey-listener failures.

## Failure -> cause -> fix

- False green gate -> PowerShell does not turn native non-zero exit codes into terminating errors -> every Cargo wrapper now checks `$LASTEXITCODE`.
- Possible stale private data in ZIP -> staging cleanup used `SilentlyContinue` -> cleanup is explicit and fatal, followed by a prohibited-file assertion.
- Wrong-window paste -> foreground focus was not revalidated and the app could capture its own HWND -> current-process windows are excluded, the target is preserved per recording/text, and foreground is checked again before Ctrl+V.
- Config/data loss -> malformed TOML and history/clipboard failures were ignored -> parse errors block overwrite and all delivery failures are surfaced while preserving `last_text`.
- Silent PTT failure -> rdev listener errors were console-only in a GUI binary -> bounded startup and runtime status channels now feed the UI.

## Verification evidence

- `scripts/check.ps1`: exit 0.
- fmt, clippy with warnings denied, tests, and release build: passed.
- `cargo test --all-targets`: 32 passed, 0 failed.
- Source secret scan: no long-form `sk-` or `xai-` patterns.
- Portable package stale-stage regression: passed; ZIP contains only the executable, quickstart, and three public documents.
- Windows 10 22H2 startup smoke: process remained alive and responsive.
- Three independent review passes closed all actionable P0/P1/P2 findings.

## Durable memory changes

- Added a repository rule requiring explicit `$LASTEXITCODE` checks after native commands in PowerShell wrappers.

## Reusable skill decision

No new skill candidate is needed. The native-exit-code lesson is small and broadly applicable, so it is recorded in the repository rule and lessons file.

## Remaining risks

- Real OpenAI/xAI credentials were not used.
- Windows 11 and cross-application PTT/clipboard/elevation smoke tests remain external acceptance work.
- Windows foreground validation has an unavoidable very small time-of-check/time-of-use window before synthesized Ctrl+V.
