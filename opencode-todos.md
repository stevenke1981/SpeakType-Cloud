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
