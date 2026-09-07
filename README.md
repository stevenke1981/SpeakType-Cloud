# SpeakType Cloud

[![Windows CI and Release](https://github.com/stevenke1981/SpeakType-Cloud/actions/workflows/windows.yml/badge.svg?branch=main)](https://github.com/stevenke1981/SpeakType-Cloud/actions/workflows/windows.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**在 Windows 文字輸入框中，按住快捷鍵說話，放開後把辨識結果貼回原視窗。**

SpeakType Cloud 是以 Rust 開發的 Windows 雲端語音輸入工具，整合 OpenAI、xAI 與 OpenRouter。使用雲端 API，不需安裝本機 Whisper、CUDA 或大型語音模型；需要網路、自己的 API Key，以及供應商可用額度。它使用全域熱鍵與剪貼簿貼上，**不是 Windows IME 核心替代品**。

## 下載與安裝

前往 **[GitHub Releases](https://github.com/stevenke1981/SpeakType-Cloud/releases)**，或直接開啟 **[最新正式發行版](https://github.com/stevenke1981/SpeakType-Cloud/releases/latest)**。目前自動建置目標為 **Windows x64**；Windows 10／11 的實機驗收狀態見下方「驗證範圍」。

| 檔案 | 用途 |
| --- | --- |
| `SpeakTypeCloud-Setup-<版本>.exe` | 一般使用者的安裝精靈，安裝於目前使用者的程式目錄。 |
| `SpeakTypeCloud-portable.zip` | 免安裝 ZIP。請完整解壓縮後執行 `SpeakTypeCloud.exe`。 |
| `SHA256SUMS.txt` | 下載檔案的 SHA-256 校驗碼。 |
| `SpeakTypeCloud.cdx.json`、`build-info.json`、`release-provenance.json` | 相依套件清單、來源 commit、建置模式與建置工具來源。 |

**免安裝不等於資料完全可攜。** 設定、歷史與金鑰仍使用 Windows 使用者目錄或 Credential Manager，不會全部儲存在 ZIP 解壓縮目錄。

未配置程式碼簽章憑證時，Release 會明確標示 **UNSIGNED**，Windows 可能顯示未知發行者警告。請核對來源與校驗碼，依自己的安全政策決定是否使用；**不要為了執行程式而關閉 Windows 安全保護**。計算下載檔案的雜湊：

```powershell
Get-FileHash .\SpeakTypeCloud-portable.zip -Algorithm SHA256
```

將結果與同一個 Release 的 `SHA256SUMS.txt` 對照。校驗碼可偵測檔案損毀，不取代可信任的程式碼簽章。

## 第一次使用

1. 啟動程式，從右上角 **「API 金鑰」** 儲存選定供應商的 Key。
2. 選擇供應商、模型、語言與麥克風；先使用預設的 **Batch PTT**。
3. 切換到記事本或其他文字輸入框，按住 **`Ctrl+Shift+Space`** 說話。
4. 放開快捷鍵，等待辨識完成。程式會嘗試貼回錄音開始時的視窗；若被 Windows 拒絕或目標已失效，請從最後辨識結果手動複製。

先在記事本確認錄音、辨識與貼上流程，再使用瀏覽器、VS Code 或 Office。處理敏感內容前，請先檢查歷史紀錄及錄音保存設定。

## 功能與模式

| 模式 | OpenAI | xAI | OpenRouter |
| --- | --- | --- | --- |
| Batch／Push-to-Talk | 已實作 | 已實作 | 已實作 |
| Realtime PTT | 已實作，需實際帳號驗證 | 已實作，需實際帳號驗證 | 不支援 |
| Continuous Dictation | 已實作，明確啟動後錄音 | 已實作，明確啟動後錄音 | 不支援 |

Batch 是預設流程。Realtime／Continuous 是選用功能，不代表所有帳號、模型與語言都已完成真實 API 驗收。供應商能力與參數請見 [API_PROVIDERS.md](API_PROVIDERS.md)。

其他現有功能包含繁體中文介面、文字後處理、可選文字剪貼簿還原、系統匣、登入自啟、單實例保護，以及本機歷史紀錄。核心流程：

```text
快捷鍵／GUI → 保存外部目標視窗 → 錄音 → 雲端辨識
           → 文字後處理 → 歷史紀錄／最後結果 → 安全檢查 → 貼上或手動複製
```

貼上前會重新檢查焦點；**沒有保存目標視窗時不會對任意前景視窗送出 Ctrl+V**。還原剪貼簿前若讀到不同文字，或無法讀取文字，就跳過還原，減少覆蓋新複製內容的風險。此保護是 best-effort，不是跨程式的原子交易；圖片與複合格式也不會完整還原。

## 金鑰、資料與隱私

GUI 儲存的 Key 使用 **Windows Credential Manager**，不寫入 `config.toml`。亦支援 `OPENAI_API_KEY`、`XAI_API_KEY`、`OPENROUTER_API_KEY` 環境變數；目前程序的環境值優先。建議一般使用者透過 GUI 設定，避免把真實 Key 寫進指令歷史、截圖或 Git。

**目前不是零音訊落地模式。** 一般 `save_wav` 選項預設關閉，但歷史功能是獨立路徑：Batch 在辨識前仍會嘗試將音訊寫入 `history/audio`，供歷史功能使用；關閉 `save_wav` 不代表不保存歷史音訊。啟動錄音後，音訊會傳送到選定供應商。設定與資料位置由 [src/paths.rs](src/paths.rs) 計算。移除程式**不會自動刪除**設定、歷史、錄音或 Credential Manager 項目；要清除 app 儲存的 Key，請先使用 GUI 的清除功能。完整說明見 [SECURITY.md](SECURITY.md)。

## 常見問題

| 狀況 | 建議檢查 |
| --- | --- |
| 無法錄音或沒有聲音 | Windows 麥克風隱私權、輸入裝置與 app 中選定的麥克風。 |
| 認證失敗／401 | Key 與供應商是否匹配、Key 是否有效、帳號是否有該模型權限。不要把 Key 貼到 issue。 |
| 額度不足／429 | 供應商額度、速率限制與帳務狀態；重試不保證立即恢復。 |
| 辨識完成卻沒有貼上 | 目標視窗是否關閉、焦點是否切換、程式權限是否不同；可先手動複製結果。 |
| 找不到程式視窗 | 檢查 Windows 系統匣；已有實例時不要反覆啟動多份。 |
| 程式內更新不可用 | 未簽章版本刻意停用自動安裝，請改用 GitHub Releases 手動下載。 |

## 開發與驗證

需要 Windows x64、Rust stable 的 MSVC toolchain、MSVC C++ Build Tools／Windows SDK，以及 **PowerShell 7.2 以上（`pwsh`）**。不要用 Windows 內建的 PowerShell 5.1 執行需要 `Test-Json` 的完整發行流程。

```powershell
git clone https://github.com/stevenke1981/SpeakType-Cloud.git
cd SpeakType-Cloud
pwsh -NoProfile -File .\scripts\check.ps1
cargo run --locked
```

`check.ps1` 依序驗證 PowerShell／發行政策、離線發行回歸測試、`cargo fmt --check`、Clippy、Rust tests 與 release build。Cargo 相依套件使用 `--locked`，native command 失敗會立即中止。

通過檢查後，可直接重用剛完成的執行檔：

```powershell
pwsh -NoProfile -File .\scripts\build-release.ps1 -SkipBuild
pwsh -NoProfile -File .\scripts\package-portable.ps1 -SkipBuild
pwsh -NoProfile -File .\scripts\generate-sbom.ps1
```

單獨建置可不加 `-SkipBuild`。需要 PDB 時，portable 腳本另加 `-IncludeSymbols`；一般使用者下載包預設不包含除錯符號。NSIS 安裝器、簽章與維護流程見 [docs/RELEASING.md](docs/RELEASING.md)。

## GitHub 自動發行

單一 [Windows CI and Release](.github/workflows/windows.yml) workflow 負責檢查、建置、打包與發布，避免重複完整編譯。

- **Push 到預設主分支**：測試與打包通過後，依 `Cargo.toml` 的 `package.version` 自動建立 `v<版本>` Release。
- **Push 相符的 `v*` tag**：執行同樣流程；tag 與 Cargo 版本不符就拒絕發布。
- **Pull request／其他分支**：只驗證並上傳 Actions artifacts，不發布 Release、不使用簽章秘密。

**同版號不覆蓋已公開的 Release。** 後續程式更新要正式發行時，請調整 `Cargo.toml` 的版本並同步 `Cargo.lock`，再推送主分支。這避免同一版本名稱下的二進位檔被悄悄替換。失敗的 draft 僅能由同一個來源 commit 重跑續傳。

沒有簽章設定也能產生供手動安裝的 Release。要啟用安全的程式內更新，須設定 signer pin 與 PFX secrets；只有有效 Authenticode、憑證 pin 與 SHA-256 都通過時才提供 updater manifest，安裝仍需使用者確認。**GitHub 自動發行不等於在使用者電腦上靜默更新。**

## 驗證範圍與文件

最新自動化結果以對應 commit 的 Actions 紀錄為準，不以文件中過往測試數量代替本次執行結果。CI 的離線 HTTP／WebSocket mocks 不使用真實 API Key。

**Windows 10／11 實體麥克風、真實 OpenAI／xAI／OpenRouter、記事本／瀏覽器／VS Code／Office 貼上，以及簽章憑證下的更新互動流程，仍需實機驗收。** CI 綠燈不能取代這些檢查。

[產品規格](SPEC.md) · [架構](ARCHITECTURE.md) · [安全與隱私](SECURITY.md) · [測試說明](TEST.md) · [待辦事項](TODOS.md) · [本次維護紀錄](docs/MAINTENANCE-2026-09-07.md) · [MIT License](LICENSE)
