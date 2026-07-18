# SpeakType Cloud

以 `speaktype` 的錄音、全域熱鍵與目前視窗注入流程為基礎，將本機 Whisper/CUDA 推論替換成 **OpenAI Speech-to-Text**、**xAI Speech-to-Text** 或 **OpenRouter** API。

## 核心流程

1. 在任何程式中按住 `Ctrl+Shift+Space`。
2. SpeakType Cloud 記住當下焦點視窗並開始錄音。
3. 放開快捷鍵後，音訊轉成 16 kHz mono WAV。
4. 依設定送往 OpenAI、xAI 或 OpenRouter。
5. 清理辨識文字，還原原視窗，使用剪貼簿 + `Ctrl+V` 注入。
6. 可選擇還原原剪貼簿內容、保留錄音、記錄文字歷史。

## API Key

程式右上角提供 **「API 金鑰」** 按鈕，可直接在 GUI 中設定 OpenAI、xAI 與 OpenRouter API Key：

1. 點選右上角「API 金鑰」。
2. 將金鑰貼到對應供應商欄位。
3. 點選「儲存 OpenAI Key」、「儲存 xAI Key」或「儲存 OpenRouter Key」。
4. 面板會顯示「已設定」狀態；也可隨時清除。

金鑰不會寫入 `config.toml`。Windows 版會將 GUI 儲存的金鑰放入 Windows Credential Manager（目標名稱 `SpeakTypeCloud:<環境變數名稱>`），並只在目前程序中載入供 provider 使用。舊版 `HKCU\Environment` 值只會複製匯入，絕不刪除或改寫，以免影響其他程式。GUI 輸入預設使用密碼遮蔽。

也可以繼續使用 PowerShell 手動設定：

```powershell
[Environment]::SetEnvironmentVariable("OPENAI_API_KEY", "sk-...", "User")
[Environment]::SetEnvironmentVariable("XAI_API_KEY", "xai-...", "User")
[Environment]::SetEnvironmentVariable("OPENROUTER_API_KEY", "sk-or-...", "User")
```

手動設定後請重新登入 Windows，或重新啟動啟動器／終端機。

## 介面

- 使用 Apple-inspired 淺色視覺：柔和灰色背景、白色卡片、較大圓角與清楚的字級層次。
- API Key 設定使用獨立浮動面板，不干擾主要錄音與辨識操作。
- Windows 啟動時自動載入可用的 CJK 系統字型，支援繁體中文介面。
- 可隱藏至 Windows 系統匣，並選擇登入時自動啟動。
- 辨識模式可選 Batch PTT（預設）、Realtime PTT 或 Continuous Dictation；即時模式只有在使用者明確開始後才會錄音。

## 開發建置

```powershell
rustup default stable
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run
```

Release：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build-release.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\package-portable.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\generate-sbom.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\build-installer.ps1 -ValidateOnly
```

NSIS 安裝器建置、Authenticode 簽署與安全更新 manifest 詳見 `scripts/`。自動更新只有在建置時提供 `SPEAKTYPE_UPDATE_SIGNER_CERT_SHA256`，且下載的 installer 通過 SHA-256、有效 Authenticode 與簽署憑證 pin 時才會啟用；未設定 trust root 時只提供手動 Releases 連結。

## 目前範圍

- Windows 10/11 優先。
- Batch PTT 保持預設；P2 另提供明確 opt-in 的 OpenAI／xAI WebSocket 即時字幕與連續聽寫。
- OpenAI 預設模型為 `gpt-4o-mini-transcribe`，可在設定中修改。
- xAI 使用 `/v1/stt`；繁體中文輸入時不送 xAI 的 `language=format` 參數，避免套用不在格式化清單中的語言碼，但仍會進行語音辨識。
- OpenRouter 使用 JSON base64 呼叫 `/v1/audio/transcriptions`；**僅支援 Batch / PTT**，不支援 Realtime 或 Continuous Dictation。
- OpenAI realtime 使用 `gpt-realtime-whisper`、24 kHz PCM 與本機 VAD；xAI realtime 使用 `/v1/stt`、16 kHz PCM，可選 Smart Turn。
- 程式不包含 API Key，也不提交編譯後 EXE、installer、SBOM output 或 `dist/` artifacts。

## 驗證狀態

- Windows 10 22H2、Rust 1.97.1 stable MSVC：fmt、Clippy、107 tests、release build 與既有 batch 啟動 smoke 通過。
- OpenAI／xAI／OpenRouter adapters 使用離線 mock server 驗證，不需要也不消耗真實 API Key。
- Windows 11、真實 provider 與 Notepad／瀏覽器／VS Code／Office 注入矩陣仍列於 `TODOS.md`，尚未宣稱通過。
- P1/P2 以離線 HTTP／WebSocket mock、VAD、音訊 backpressure、取消、更新供應鏈與 release static tests 驗證；真實 realtime provider、實體麥克風與安裝器互動 smoke 仍待外部環境。

詳見 `SPEC.md`、`ARCHITECTURE.md`、`SECURITY.md` 與 `TEST.md`。
