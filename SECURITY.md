# Security and Privacy

## API Key

- 可從 GUI 的「API 金鑰」面板設定 OpenAI 與 xAI 金鑰。
- GUI 輸入預設以密碼模式遮蔽，儲存成功後會立即清空輸入欄位。
- Windows 版將金鑰保存於目前使用者的環境變數登錄位置 `HKCU\Environment`，並同步到目前程式的 process environment。
- `config.toml` 僅保存環境變數名稱，永遠不保存 API Key 本身。
- 仍支援外部設定 `OPENAI_API_KEY` 或 `XAI_API_KEY`；若目前 process environment 已有值，程式會優先使用該值。
- GUI 提供清除操作，會同時移除使用者環境變數與目前 process environment 中的值。
- 錯誤訊息最多保留 800 個 API response 字元，不記錄 request headers。
- 使用者環境變數不是加密的秘密儲存區；具有相同 Windows 使用者權限的程式可能讀取它。
- 後續若需要更高安全性，可改用 Windows Credential Manager 或 DPAPI；不要改成明文 TOML。

## 音訊

- WAV 保存預設關閉。
- 只有明確按住／切換快捷鍵的錄音區段會送往選定供應商。
- 開啟保存後檔案位於 `%LOCALAPPDATA%\SpeakType\SpeakTypeCloud\data\recordings`（實際路徑依 `directories` crate 決定）。

## 視窗與剪貼簿

- 只保存 HWND 數值於記憶體，不保存視窗標題或內容。
- 可選擇貼上後還原先前「文字」剪貼簿；圖片或其他複合格式不會完整還原。
- 高權限視窗可能拒絕低權限程式注入，失敗時改為複製文字。

## 威脅模型

- 惡意本機程式仍可能讀取使用者層級環境變數、程序記憶體或剪貼簿。
- 公共電腦不應保存永久 API Key；使用完畢後應從 GUI 清除。
- 企業環境應使用受限 API project、支出上限、最小權限與端點代理。
