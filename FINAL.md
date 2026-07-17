# SpeakType Cloud v1 Delivery

交付日期：2026-07-17<br>
公開來源：https://github.com/stevenke1981/SpeakType-Cloud

## What changed

- 完成 Windows Rust／egui 語音輸入流程，包含 cpal 錄音、16 kHz mono PCM16 WAV、全域 PTT／切換錄音、OpenAI 與 xAI batch STT adapters、後處理、history 與剪貼簿注入。
- OpenAI／xAI providers 已以離線 mock server 獨立測試 timeout、401／429、空結果、API Key 遮蔽與 multipart request；xAI `file` 欄位順序已鎖定為最後。
- 焦點注入會排除 SpeakType 自身 HWND，保存錄音與最後文字的原始外部目標，送出 Ctrl+V 前再次驗證 foreground；失敗保留 `last_text` 並如實回報 fallback copy 結果。
- malformed `config.toml` 不再被預設值靜默覆蓋；history、clipboard 與 hotkey listener 失敗皆可由 UI 觀察。
- PowerShell gate 逐步檢查 native exit code；portable staging 清除失敗會中止，並在壓縮前拒絕 config、`.env`、history、WAV 與 log。

## Why

原始開發包只有未經完整實機驗證的 MVP 骨架。這次工作把「可編譯」提升為可重現的自動驗收，並修正會造成假成功、資料遺失、錯窗貼上、秘密掃描假警報與核心 PTT 靜默失效的發布阻塞。

## Verification

- Windows 10 22H2（10.0.19045）、Rust 1.96.0 stable MSVC。
- `scripts/check.ps1`：exit 0。
- fmt、clippy `-D warnings`、32 tests、release build：全數通過。
- source secret scan：未發現長格式 `sk-`／`xai-` 內容。
- portable stale-stage regression：通過。
- release EXE 啟動 3 秒後仍存活且 `Responding=True`。
- 三輪獨立 code review：所有 actionable P0／P1／P2 findings 已關閉，最終 verdict 為 release-ready。

詳細、可重現的命令與限制見 `TEST.md`。

## Local artifact

- Portable ZIP：`E:\SpeakType-Cloud\dist\SpeakTypeCloud-portable.zip`
- SHA-256：`0BDF13FDED46FC922D4071D38138573BD2E21F49A7FE38DB63161D2EB381642C`

`dist/` 依安全契約不提交 Git；公開 repo 提供可重現建置腳本。

## Remaining external acceptance

- 未使用使用者的真實 OpenAI／xAI API Key，因此沒有付費端到端請求證據。
- Windows 11、Notepad／瀏覽器／VS Code／Office、剪貼簿被占用與高低權限視窗矩陣仍待實機 smoke。
- Windows foreground 驗證與實際送鍵之間仍有系統層級不可完全消除的極短競態。

## Durable improvement

- `AGENTS.md` 現在明定 PowerShell native command 必須逐步檢查 `$LASTEXITCODE`。
- `.codex/evolution/reviews/` 與 `LESSONS.md` 保存本次可追溯證據與可重用教訓。
