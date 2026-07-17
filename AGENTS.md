# AGENTS.md

## Mission

將 SpeakType Cloud 完成為可發布的 Windows 語音輸入法。優先保證：資料不遺失、API Key 不外洩、焦點視窗注入可靠、OpenAI/xAI adapters 可獨立測試。

## Required Workflow

1. 先讀 `SPEC.md`、`ARCHITECTURE.md`、`SECURITY.md`、`TODOS.md`。
2. 每次只處理一個明確工作項。
3. 先補或更新測試，再改實作。
4. 完成後執行 `scripts/check.ps1`。
   PowerShell 呼叫 Cargo 等 native command 時，每一步都必須檢查 `$LASTEXITCODE`，不得只依賴 `$ErrorActionPreference`。
5. 不得提交 API Key、錄音、history、config.toml 或 dist artifacts。
6. 不得以刪除大量檔案、force push、重寫主分支解決問題。

## Architecture Boundaries

- `providers/` 不得依賴 egui、hotkey 或 injector。
- cpal callback 不得執行網路、檔案或 UI 操作。
- API Key 不得進入 config serialization、Debug output 或 error body。
- UI 不直接建立 reqwest request；使用 transcription worker。
- 注入失敗必須保留可複製的最後文字。

## Definition of Done

- fmt、clippy、tests、release build 通過。
- 新功能有錯誤路徑與取消／timeout 考量。
- 文件與設定範例同步。
- Windows 10/11 至少各做一次 smoke test。
