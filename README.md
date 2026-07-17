# SpeakType Cloud

以 `speaktype` 的錄音、全域熱鍵與目前視窗注入流程為基礎，將本機 Whisper/CUDA 推論替換成 **OpenAI Speech-to-Text** 或 **xAI Speech-to-Text** API。

## 核心流程

1. 在任何程式中按住 `Ctrl+Shift+Space`。
2. SpeakType Cloud 記住當下焦點視窗並開始錄音。
3. 放開快捷鍵後，音訊轉成 16 kHz mono WAV。
4. 依設定送往 OpenAI 或 xAI。
5. 清理辨識文字，還原原視窗，使用剪貼簿 + `Ctrl+V` 注入。
6. 可選擇還原原剪貼簿內容、保留錄音、記錄文字歷史。

## API Key

程式右上角提供 **「API 金鑰」** 按鈕，可直接在 GUI 中設定 OpenAI 與 xAI API Key：

1. 點選右上角「API 金鑰」。
2. 將金鑰貼到對應供應商欄位。
3. 點選「儲存 OpenAI Key」或「儲存 xAI Key」。
4. 面板會顯示「已設定」狀態；也可隨時清除。

金鑰不會寫入 `config.toml`。Windows 版會將它保存於目前使用者的環境變數（`HKCU\Environment`），並同步到目前執行中的程式。GUI 輸入預設使用密碼遮蔽。

也可以繼續使用 PowerShell 手動設定：

```powershell
[Environment]::SetEnvironmentVariable("OPENAI_API_KEY", "sk-...", "User")
[Environment]::SetEnvironmentVariable("XAI_API_KEY", "xai-...", "User")
```

手動設定後請重新登入 Windows，或重新啟動啟動器／終端機。

## 介面

- 使用 Apple-inspired 淺色視覺：柔和灰色背景、白色卡片、較大圓角與清楚的字級層次。
- API Key 設定使用獨立浮動面板，不干擾主要錄音與辨識操作。
- Windows 啟動時自動載入可用的 CJK 系統字型，支援繁體中文介面。

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
```

## 目前範圍

- Windows 10/11 優先。
- v1 使用「放開後批次辨識」，不是即時逐字串流。
- OpenAI 預設模型為 `gpt-4o-mini-transcribe`，可在設定中修改。
- xAI 使用 `/v1/stt`；繁體中文輸入時不送 xAI 的 `language=format` 參數，避免套用不在格式化清單中的語言碼，但仍會進行語音辨識。
- 程式不包含 API Key，也不包含編譯後 EXE。

## 驗證狀態

- Windows 10 22H2、Rust 1.96.0 stable MSVC：fmt、clippy、32 tests、release build 與啟動 smoke 曾通過。
- OpenAI／xAI adapters 使用離線 mock server 驗證，不需要也不消耗真實 API Key。
- Windows 11、真實 provider 與 Notepad／瀏覽器／VS Code／Office 注入矩陣仍列於 `TODOS.md`，尚未宣稱通過。
- GUI API Key 與新版主題加入後需要重新執行上述 Windows 建置與 smoke test。

詳見 `SPEC.md`、`ARCHITECTURE.md`、`SECURITY.md` 與 `TEST.md`。
