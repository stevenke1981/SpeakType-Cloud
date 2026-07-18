# Test and Acceptance Record

驗證日期：2026-07-18<br>
驗證主機：Windows 10 22H2（10.0.19045）、Rust 1.97.1 stable MSVC

## Automated gate

執行：

```powershell
$env:PATH = "C:\Users\steven\.cargo\bin;$env:PATH"
.\scripts\check.ps1
```

結果：

- `cargo fmt --check`：通過。
- `cargo clippy --all-targets --all-features -- -D warnings`：通過。
- `cargo test --all-targets`：107 passed、0 failed。
- `cargo build --release`：通過。
- `scripts/check.ps1`：exit 0。

自動測試涵蓋：

- config 預設值、provider key 選擇、環境變數名稱驗證、malformed TOML 保護，以及 serialization／Debug 不讀入 API Key 值；
- stereo-to-mono、16 kHz 重採樣、WAV RIFF 輸出與 cpal callback 錯誤交接；
- hotkey parsing 與 CJK spacing cleanup；
- hotkey listener bounded startup handshake、立即失敗與 runtime failure UI channel；
- OpenAI endpoint、必要 multipart 欄位、timeout、401、空結果與 response body secret redaction；
- xAI timeout、429、空結果、中文 formatting 行為，以及 `file` 為 multipart 最後欄位；
- OpenRouter JSON base64 endpoint、必要欄位、timeout、401、429、5xx、空結果、response body secret redaction 與 realtime 拒絕；
- 排除 SpeakType 自身 HWND、保存最後外部視窗與原始文字目標，並在無效／錯誤焦點 HWND 時禁止注入；
- history 寫入失敗、fallback clipboard 失敗與多重非致命錯誤不得靜默遺失。
- Windows Credential Manager 匯入、外部環境變數保留、空白 credential 與遷移失敗不遺失 key；
- 系統匣 close／exit 決策、登入自啟 registry transaction 與 hotkey runtime rollback；
- 408／429／5xx 有界 backoff、真正中止 in-flight async HTTP、JobId／channel stale-result 隔離；
- 中文標點、OpenCC 繁簡、非遞迴詞典與完整匹配語音命令；
- OpenAI／xAI mock WebSocket handshake、協定欄位、auth redaction、partial／final、commit ordering、Smart Turn 三態與大小限制；
- 固定容量 live audio、backpressure/drop 統計、44.1 kHz stateful resampling、固定 10 ms VAD frame、pre-roll／silence／max endpoint；
- Realtime PTT／Continuous 狀態、取消與 worker join、明確 batch fallback；
- 更新 URL allowlist、chunked response 上限、SHA-256、Authenticode signer pin 與三階段 UI gate。

## Release and package checks

- `scripts/check.ps1` 已以模擬 native exit code 37 驗證會在第一個失敗 gate 正確停止。
- `scripts/build-release.ps1` 已以模擬 native exit code 41 驗證不會把失敗建置當成成功。
- `scripts/package-portable.ps1` 已以 staging 中預放 stale `config.toml` 做回歸測試；重新打包後該檔未進入 ZIP。
- portable ZIP 僅含 `SpeakTypeCloud.exe`、`QUICKSTART.txt`、README、SECURITY 與 API provider 文件。
- source secret scan 未發現符合長格式 `sk-`／`xai-` 的內容。
- `scripts/test-release.ps1`、NSIS template、SBOM schema、PowerShell syntax 與 workflow static checks 通過。
- CycloneDX SBOM 連續兩次生成 SHA-256 相同。
- NSIS 3.12.0 與 `nsis.install` nupkg 使用 repo-tracked SHA-256 固定；GitHub Actions 使用完整 commit SHA。
- 本機有 `signtool.exe`，但沒有 code-signing certificate；unsigned verify 會拒絕，未宣稱已簽。

## Windows 10 smoke

- `SpeakTypeCloud.exe` 啟動 3 秒後仍存活且 `Responding=True`，通過啟動 smoke。
- 本輪未提供真實 OpenAI／xAI／OpenRouter API Key，因此未送出付費請求。

## Pending external acceptance

以下項目需要額外環境或使用者憑證，尚未宣稱通過：

1. Windows 11 完整 gate 與啟動 smoke。
2. 以真實 OpenAI、xAI 與 OpenRouter Key 做端到端錄音／辨識。
3. Notepad、Chrome／Edge、VS Code、Word／Excel 的實際 Ctrl+V 注入矩陣。
4. 提升權限與非提升權限視窗之間的注入限制。
5. 圖片／複合格式剪貼簿還原行為；目前設計只保證文字剪貼簿。
6. 真實 OpenAI／xAI realtime、OpenRouter batch、實體麥克風、tray 點擊、Credential Manager／HKCU Run 與 NSIS 安裝／更新 smoke。
