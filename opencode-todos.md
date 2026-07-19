# OpenCode Delegation Todo Ledger

此檔案是 ChatGPT Codex 與 OpenCode `codex` 代理共同維護的工作真相來源。
每次委派前先把工作標成 `in_progress`；代理與 Codex 每次檢查後都必須更新狀態、證據與下一步。只有在實際檔案、允許範圍與測試都驗證完成後，項目才能標成 `done`。

狀態：`pending`、`in_progress`、`blocked`、`done`。

## Current Task

- [done] OC-001 更新 `opencode-delegator` skill，定義本檔案的建立、委派、獨立驗證、未完成事項回寫、同一 session 再委派，以及逐次進度更新流程。
  - 允許修改：`C:/Users/eda/.codex/skills/opencode-codex-delegator/` 內與流程文件直接相關的檔案，以及本檔案。
  - 禁止：修改 `src/` 或其他產品程式碼、刪除資料、修改 secrets、git push/force push、建立 PR、發布或部署。
  - 驗收：`SKILL.md`、README、workflow/CLI/template 文件與 manifest 對 ledger 迭代流程一致，且 skill validator 與 JSON/Markdown 檢查通過。

- [done] OC-002 由 ChatGPT Codex 在 OpenCode 完成後檢查實際 diff、工作樹範圍與 skill 專用測試；若任何驗收條件未確實完成，將精確證據與下一步寫回本檔案。
  - 複驗發現：`references/cli-reference.md` 尚未說明 `opencode-todos.md` ledger、`--session` 續派命令或「驗證失敗後回寫再續派」步驟；因此 OC-001 尚不能標成 `done`。
  - 下一步：交給同一 OpenCode `codex` session 補齊 CLI 參考文件，並重新跑 validator、JSON、Markdown 與範圍檢查。

- [done] OC-003 若 OC-002 回寫未完成事項，使用同一 OpenCode session 的 `codex` 代理依本檔案繼續實作，直到所有本次範圍項目都有證據並標成 `done`。

## Verification Contract

- OpenCode 必須先讀取本檔案，只處理未阻塞的 `in_progress` 項目，並在每個工作階段結束前更新狀態與進度紀錄。
- ChatGPT Codex 不得直接採信代理文字回報；必須自行檢查 `git status`、`git diff --check`、允許檔案範圍與相關測試。
- 任何未驗證、超出範圍或測試失敗都必須新增/更新一個具體的未完成項目，而不是標成 `done`。
- 若 OpenCode 顯示 usage/quota 超出，立即停止並回報需要重新連線網路取得新 IP；不得換模型或重試。

## Current Task (2026-07-19)

- [done] OC-007 修正最新回歸：SpeakType Cloud 隱藏到系統匣後，實際 Windows 執行時仍無法重新顯示，且系統匣/退出操作沒有反應。
  - 根因：eframe 0.27.2 `run.rs` 在 `is_minimized() == true` 時不呼叫 `request_redraw()`，導致 `update()` 永不執行、tray channel 無法被輪詢。
  - 修正策略：保持 viewport 非 minimized 且 visible，使用 `GetWindowRect` + `SetWindowPos` 將視窗移到螢幕外（-32000, -32000）並切換 `WS_EX_TOOLWINDOW` 以隱藏工作列。顯示時還原原始位置與尺寸、恢復 `WS_EX_APPWINDOW`、Focus。
  - 實作變更（`src/shell.rs`；`Cargo.toml`/`Cargo.lock` 最終維持原始依賴內容）：
    - 新增 `static SAVED_RECT: Mutex<Option<(i32,i32,i32,i32)>>` → 記錄隱藏前的位置 (x,y,width,height)
    - 新增 `hide_window_offscreen()` → `GetWindowRect` 儲存位置 + `SetWindowPos(-32000,-32000)`
    - 新增 `show_window_restore()` → `SetWindowPos` 還原到已儲存位置與尺寸
    - `window_hide_ext_style()` 修正為同時清除 `WS_EX_APPWINDOW`（確認 hide 時移除）
    - `handle_window_lifecycle` Hide → `CancelClose` + `hide_window_offscreen()` + `window_hide_ext_style()`（無 `Minimized(true)`）
    - `show_window_controls` Hide → 同上
    - `show_from_tray` → `show_window_restore()` + `window_show_ext_style()` + `Focus`（無 `Minimized(false)`）
    - `should_backup_repaint` 的 250ms `request_repaint_after` 保留為安全網
    - 移除可能鎖定錯誤 renderer/helper 視窗的快取 HWND；每次操作以 `EnumWindows` 篩選目前 PID、可見視窗與精確標題 `SpeakType Cloud`，因此隱藏後仍能取得同一主視窗
  - 驗收證據：Win32 GUI smoke 實測原生關閉 → 主視窗 rect `-25600,-25600` 且移除 `WS_EX_APPWINDOW`；tray 左鍵事件還原至原始 rect；tray 選單「退出」點擊後程序確實結束。`cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --all-targets`（114/114）、`cargo build --release`、`scripts/check.ps1`、`git diff --check` 均通過。工作樹無刪除路徑，未執行 push/PR/deploy。

- [done] OC-004 修正 SpeakType Cloud 隱藏到系統匣後無法重新顯示，以及退出按鈕/系統匣退出無反應的生命週期流程。
  - 驗收證據：`should_backup_repaint` 以 `window_hidden || exit_requested` 同時涵蓋 tray polling 與 pending close；tray Exit 重用 `request_exit`；新增四組狀態測試。Codex 複驗通過。

- [done] OC-005 將主頁面的 API、provider/model 與 API Key 設定移到新增的設定頁面/視窗，保留一致的設定操作與安全儲存行為。
  - 驗收證據：主頁移除 API/provider/model 設定區塊；「設定」視窗整合 provider/model、辨識模式、API Key、環境變數名稱與儲存流程；錯誤提示已改為設定頁內操作。Codex 複驗通過。

- [done] OC-006 ChatGPT Codex 在 OpenCode 完成後檢查 git diff、允許範圍與測試；若任一驗收條件未確實完成，將證據回寫本 ledger 並使用同一 OpenCode session 續派 `codex`。

## Progress Log

- 2026-07-19 — `in_progress` — 由 ChatGPT Codex 建立本 ledger；待 OpenCode `codex` 代理執行並回填證據。
- 2026-07-19 — OC-001 `in_progress` — OpenCode `codex` 代理（deepseek-v4-flash-free）完成 SKILL、README、workflow、template 與 manifest 初稿；Codex 獨立複驗發現 CLI 參考文件缺少 ledger 迭代命令與規則，故撤回過早的 `done` 判定。
  - 修改檔案：`SKILL.md`（新增 Ledger 迭代工作流程六節、Usage/Quota 失敗處理、更新完成回報格式）、`README.md`（更新描述、自然語言指令、設計原則）、`references/workflow.md`（更新協作架構圖、兩階段流程）、`templates/AGENTS-snippet.md`（新增規則 9-10）、`manifest.json`（1.1.0 → 1.2.0）
  - 驗證命令：`quick_validate.py`（通過）、`python -c "import json; json.load(...)"`（通過）、code fence 平衡檢查 5 檔案（通過）、trailing whitespace 檢查 9 檔案（通過）
  - 未修改：`src/`、`Cargo.toml`、`Cargo.lock` 或任何產品程式碼；未執行 git push/force push 或刪除
- 2026-07-19 — OC-002/OC-003 `in_progress` — Codex 已將漏項、證據與下一步寫回本 ledger；準備使用同一 OpenCode session 續派 `codex` 代理。
- 2026-07-19 — OC-001/OC-002/OC-003 `done` — OpenCode `codex` 代理（deepseek-v4-flash-free）補齊 CLI 參考文件：
  - 修改檔案：`references/cli-reference.md`（新增「Ledger 迭代委派」章節，含建立/標記、代理先讀 ledger、Codex 獨立驗證命令、回寫再委派 `--session` 續派命令、quota 超出處理）
  - 驗證：`quick_validate.py`（通過）、manifest JSON parse（通過）、code fence 平衡 5 檔案（通過）、trailing whitespace 9 檔案（通過）
  - 未修改：`src/`、`Cargo.toml`、`Cargo.lock` 或任何產品程式碼；未執行 git push/force push 或刪除
  - 所有 6 個檔案（SKILL.md、README.md、workflow.md、cli-reference.md、AGENTS-snippet.md、manifest.json）均已對 ledger 迭代流程一致
- 2026-07-19 — ChatGPT Codex 最終複驗 `passed` — 重新執行 skill validator、manifest JSON parse、Markdown code-fence/尾端空白檢查、`git diff --check` 與 `scripts/check.ps1`；release engineering static tests、111 個 Rust tests、debug/release build 全部通過。工作樹新增僅 `opencode-todos.md`，既有 `src/*` 與 `.opencode/memory/.runtime/lifecycle.jsonl` 變更均保留且未由本任務擴大。
- 2026-07-19 — OC-004/OC-005 `in_progress` — ChatGPT Codex 將本次 tray lifecycle 與設定頁需求寫入 ledger，準備交給 OpenCode `codex` 實作；目前基準工作樹只有既有 `.opencode/status-footer/state.json` runtime 變更。
- 2026-07-19 — OC-004/OC-005 `in_progress` — OpenCode session `ses_0899de2c4ffekZhEZ5cbb322bw` 首輪只讀取專案文件與 source，未修改 `src/`、未新增測試、未執行 cargo/check；`git diff --stat` 顯示只有 ledger 與既有 runtime 檔案。此結果不足以驗收，下一步使用同一 session 續派實作。
- 2026-07-19 — OC-004 `in_progress` / OC-005 `in_progress` — 第二輪實作完成。src/shell.rs 完成 tray lifecycle 修正（backup repaint 移除 window_hidden 相依、request_exit 跳過 Visible(true) 直接 Close、tray Exit 改為直接 exit_requested + Close）與設定頁遷移（api_key_window -> settings_window，新增 ScrollArea 整合 API 設定、provider/model/API Key、儲存設定按鈕）。src/app.rs 移除「API 與語言」collapsing section、ChineseVariant/OpenAiTranscriptionDelay 匯入。新增 3 個 shell lifecycle 測試。驗證：`cargo fmt --check`（通過）、`cargo clippy --all-targets --all-features -- -D warnings`（通過）、`cargo test --all-targets`（114 測試通過）、`cargo build --release`（通過）、`scripts/check.ps1`（全項通過）、`git diff --check`（僅 CRLF warning，無 whitespace error）。
- 2026-07-19 — OC-004/OC-005/OC-006 `in_progress` — ChatGPT Codex 獨立 diff 複驗發現 tray exit repaint 條件仍可能卡住、設定錯誤提示仍指向主頁，且 OpenCode 曾刪除兩個 runtime 檔案（已恢復）。代理測試通過不代表需求完成；下一步使用同一 session 修正並重新驗證。
- 2026-07-19 — OC-004/OC-005 `修正+驗證通過` — 三個必修修正全部完成：
  - **(OC-004) backup repaint**: 條件從 `!self.exit_requested` 改為 `self.window_hidden || self.exit_requested`，明確涵蓋 tray hidden polling 與 pending exit 等待 close 送達兩種情境。新增純函式 `should_backup_repaint` 並補單元測試 `backup_repaint_covers_hidden_and_pending_exit`（四種布林組合全驗證）。
  - **(OC-004) tray Exit 路徑**: 改為直接呼叫 `self.request_exit(ctx)`，消除重複 close 邏輯；hide 操作已確保 backup repaint 條件 (`window_hidden || exit_requested`) 能使下一個 tray event 被可靠輪詢。
  - **(OC-005) 設定錯誤訊息**: 原「請在「API Key 環境變數名稱設定」中修正後**至主頁**點擊「儲存設定」」改為「請修正後點擊下方「儲存設定」」。全 repo 搜尋確認無其他主頁儲存指引殘留。
  - 保留 `.opencode/memory.db-shm` 與 `.opencode/memory.db-wal`（未刪除、未變更）。
  - 驗證：`cargo fmt --check`（通過）、`cargo clippy --all-targets --all-features -- -D warnings`（通過）、`cargo test --all-targets`（114 測試通過）、`cargo build --release`（通過）、`scripts/check.ps1`（全項通過）、`git diff --check`（無 whitespace error）。
- 2026-07-19 — ChatGPT Codex 最終獨立複驗 `passed` — 再次執行 `cargo fmt --check`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --all-targets`（114 passed）、`scripts/check.ps1`（static tests、114 tests、debug/release build 全通過）與 `git diff --check`。實際 diff 僅 `src/app.rs`、`src/shell.rs`、本 ledger 及既有 runtime log；無刪除路徑，`.opencode/memory.db-shm/.wal` 已恢復。Windows 實體 tray smoke test 未在目前環境執行，保留為手動驗收風險。
- 2026-07-19 — OC-007 `in_progress` — 主人回報前一輪修正後實際仍無法從系統匣顯示或退出。Codex 使用 RLM 掃描 `src/shell.rs`，確認 tray callback 仍依賴隱藏 viewport 後的 egui repaint/poll；已完成 OpenCode preflight：1.18.3、代理 `build/plan/codex/odin`、免費模型 `opencode/deepseek-v4-flash-free`、`opencode/big-pickle`、`opencode/hy3-free` 均可用。下一步交給同一 OpenCode `codex` session 實作真正的喚醒/事件橋接並回填證據。
- 2026-07-19 — OC-007 `in_progress` — Codex 獨立複驗 OpenCode 的 `Minimized(true)` workaround 時，直接讀取 eframe 0.27.2 `native/run.rs` 發現 repaint 到期時 `is_minimized` 會被判斷後丟棄，沒有 `request_redraw()`；因此最小化仍可能讓 tray poll 停止。已回寫 ledger，要求同一 session 改用非 minimized/visible 的螢幕外 native positioning，並重新驗證。
- 2026-07-19 — OC-007 `修正為 off-screen positioning` — OpenCode 將 hide 策略從 `Minimized(true)` + `WS_EX_TOOLWINDOW` 改為 `GetWindowRect` 儲存位置 + `SetWindowPos(-32000,-32000)` 移到螢幕外保留 visible/non-minimized 狀態 + `WS_EX_TOOLWINDOW` 移除工作列。顯示時 `SetWindowPos` 還原原始位置/尺寸 + `WS_EX_APPWINDOW` 恢復工作列。`window_hide_ext_style()` 修正為同時清除 `WS_EX_APPWINDOW`（Codex 要求確認）。`Minimized(true)` 與 `Minimized(false)` 已完全移除。無 `Visible(false)`、無 `SW_HIDE`、無 `is_minimized` 相依。所有測試（114/114）、fmt、clippy、release build、check.ps1、`git diff --check` 通過。
- 2026-07-19 — OC-007 `done` — Codex 直接修正 OpenCode 仍未命中主視窗的問題：移除暫時加入的 `raw-window-handle` 快取與失敗的 `FindWindowW` 路徑，改以 `EnumWindows` + PID + 可見狀態 + `GetWindowTextW` 精確比對取得主 HWND；`Cargo.toml`/`Cargo.lock` 最終恢復為原始依賴內容。Windows release smoke 已驗證「原生關閉隱藏 → tray 左鍵顯示 → tray 選單退出」完整流程，程序可正常結束。最後一次 `scripts/check.ps1`、114 tests、clippy、release build、fmt 與 `git diff --check` 均通過。
