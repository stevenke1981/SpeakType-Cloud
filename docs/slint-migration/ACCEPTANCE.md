# Acceptance and regression matrix

All candidate tests are NOT_RUN at checkpoint 01. Source inventory means implemented in baseline, not hardware/provider verification.

| ID | Capability / baseline | Candidate acceptance / required evidence |
|---|---|---|
| AC01 | egui desktop | Rust + Slint compiled, real window screenshot, CJK, keyboard, resize/DPI, no hidden egui runtime |
| AC02 | Batch PTT/toggle, CPAL, gain/device/limits, WAV | Start → stop → process → preview with synthetic audio; real capture remains NOT_RUN without private-audio permission; min/max/error tests |
| AC03 | OpenAI, xAI, OpenRouter batch | Preserve provider settings and offline request/response regressions |
| AC04 | Gemini absent | User enters masked key; save through Credential Manager only on explicit action; mock verifies one official audio schema, header, request limits and response parser |
| AC05 | cancellation/retry/JobId | Cancel capture/work, reject stale completion, retry selected failure, no duplicate output; 401/429/timeout/empty/blocked cases |
| AC06 | Postprocess, dictionary, voice commands | Raw/final editable preview, Chinese conversion/punctuation, deterministic cleanup; commands remain opt-in and cannot execute generated code |
| AC07 | target capture, guarded paste/fallback | Explicit preview confirmation, stale/missing target rejected, text retained after failure; automated tests never type into private apps; dedicated disposable test target for approved real paste |
| AC08 | Global PTT/toggle hotkeys | Registration failure visible, settings rollback, tray-hidden events responsive; real key flow tested without microphone/network in demo |
| AC09 | tray show/exit, single instance | Minimize/close hides only when tray exists; tray restore and exit operate; failed tray permits normal close; no orphan recording worker |
| AC10 | History JSONL/audio/play/delete/retention | Separate text/audio opt-ins; default no persistent content; actual path-controlled tests prove no files when off and bounded retention when on; failure/cancel no orphan WAV |
| AC11 | Key environment/Credential Manager | Never inspect existing secrets in task; fake key Debug/config/log redaction tests, demo never loads credentials; key input clears on save |
| AC12 | OpenAI/xAI realtime + VAD/continuous | Preserve UI reachability/config and ordered final/stale-cancel tests; external streaming/hardware NOT_RUN without approval |
| AC13 | Startup + secure updater/model list | Preserve transactional settings, explicit actions, HTTPS/hash/signature/signer pin; no runtime reads or update network in demo |
| AC14 | NSIS/portable/SBOM/checksums/signing | Add Inno Setup, dependency inspection, per-user install, launch, shortcuts, normal uninstall preserving test user data; record exact binary/installer hashes |
| AC15 | baseline CI/release | scripts/check.ps1 pass; independent review on immutable SHA; current CI PASS; screenshots and interactive evidence; immutable new 0.2.0 release assets, no old asset overwrite |
| AC16 | Platform scope | Windows 11 local and Windows 10 separate smoke required by AGENTS; any missing platform explicitly NOT_RUN and release decision escalated |
| AC17 | Privacy consent | Google disclosure + deliberate opt-in before sending; no zero-retention/no-training blanket claims; default safe preview and persistence |
| AC18 | SVG icons (new user requirement) | Bundled licensed/original SVG, consistent stroke/size/color, text/tooltips, distinct non-color states; actual 100/150/200% DPI; tray ICO/RGBA derivative evidence |
| AC19 | Third-party license | Exact Slint license route and notices checked, required AboutSlint attribution present where applicable; no assumed MIT dependency licensing |

## Typeless comparison scope

| Feature family | Baseline / initial candidate scope |
|---|---|
| Dictation | Existing batch/realtime core; Slint migration and Gemini batch are primary scope |
| Filler removal / formatting | Existing deterministic punctuation/dictionary; preview/edit; model rewrite quality is not claimed |
| Translate | Not currently inventoried as dedicated workflow; gap, no parity claim |
| Selected-text edit / ask | Not currently implemented; gap |
| Cross-site search / contextual actions | Not currently implemented; gap; no browser/private-app inspection |
| Personal vocabulary | Existing dictionary preserved; no implicit learning/training claim |
| Privacy / retention | Local explicit opt-ins; Google BYOK terms apply; not equivalent to vendor-hosted privacy guarantees |
| Mobile/macOS | Out of Windows release scope |

Official competitive research supplied by parent will refine this matrix. It must not broaden implementation silently or turn mock success into provider/hardware success.
