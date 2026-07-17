# Security and Privacy

## API Key

- 只從 `OPENAI_API_KEY` 或 `XAI_API_KEY` 讀取。
- `config.toml` 僅保存環境變數名稱。
- 錯誤訊息最多保留 800 個 API response 字元，不記錄 request headers。
- 正式版可再加入 Windows Credential Manager；不要改成明文 TOML。

## 音訊

- WAV 保存預設關閉。
- 只有明確按住／切換快捷鍵的錄音區段會送往選定供應商。
- 開啟保存後檔案位於 `%LOCALAPPDATA%\SpeakType\SpeakTypeCloud\data\recordings`（實際路徑依 `directories` crate 決定）。

## 視窗與剪貼簿

- 只保存 HWND 數值於記憶體，不保存視窗標題或內容。
- 可選擇貼上後還原先前「文字」剪貼簿；圖片或其他複合格式不會完整還原。
- 高權限視窗可能拒絕低權限程式注入，失敗時改為複製文字。

## 威脅模型

- 惡意本機程式仍可能讀取使用者層級環境變數或剪貼簿。
- 公共電腦不應設定永久 API Key。
- 企業環境應使用受限 API project、支出上限、最小權限與端點代理。
