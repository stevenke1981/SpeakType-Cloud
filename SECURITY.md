# Security and Privacy

## API Key

- 可從 GUI 的「API 金鑰」面板設定 OpenAI、xAI 與 OpenRouter 金鑰。
- GUI 輸入預設以密碼模式遮蔽，儲存成功後會立即清空輸入欄位。
- Windows 版將 GUI 儲存的金鑰保存於 Windows Credential Manager generic credential，目標名稱為 `SpeakTypeCloud:<環境變數名稱>`；啟動後僅載入目前程式的 process environment。
- `config.toml` 僅保存環境變數名稱，永遠不保存 API Key 本身。
- 仍支援外部設定 `OPENAI_API_KEY`、`XAI_API_KEY` 或 `OPENROUTER_API_KEY`；若目前 process environment 已有值，程式會優先使用該值。
- 舊版位於 `HKCU\Environment` 的標準金鑰只會複製匯入 Credential Manager；程式絕不刪除或改寫外部環境變數。
- GUI 清除只移除 SpeakType Cloud 的 Credential Manager 項目與目前 process environment，不影響其他程式的永久環境設定。
- 錯誤訊息最多保留 800 個 API response 字元，不記錄 request headers。
- Credential Manager 將秘密限制在目前 Windows 使用者內容中；具有相同使用者權限的惡意程式仍可能存取程序記憶體或使用者憑證。

## 音訊

- WAV 保存預設關閉。
- 只有明確按住／切換快捷鍵的錄音區段會送往選定供應商。
- 開啟保存後檔案位於 `%LOCALAPPDATA%\SpeakType\SpeakTypeCloud\data\recordings`（實際路徑依 `directories` crate 決定）。
- Realtime 與 Continuous 模式預設關閉；只有使用者明確啟動 session 後才將音訊送出，停止／取消後等待 worker 確認結束。

## 更新供應鏈

- 更新器只接受本專案 GitHub Releases 的 HTTPS URL，限制回應大小並驗證 SHA-256。
- 自動啟動 installer 前必須同時通過 Authenticode `Valid` 與建置時固定的 leaf certificate DER SHA-256；缺少 pin 時更新器停用。
- 一般 CI 只產生 unsigned artifacts，不產生可供自動更新使用的 manifest；正式 tag 流程必須先簽章，再對簽後 bytes 產生 manifest。
- 更新不使用 silent install，必須由使用者明確確認啟動安裝精靈。

## 視窗與剪貼簿

- 只保存 HWND 數值於記憶體，不保存視窗標題或內容。
- 可選擇貼上後還原先前「文字」剪貼簿；圖片或其他複合格式不會完整還原。
- 高權限視窗可能拒絕低權限程式注入，失敗時改為複製文字。

## 威脅模型

- 惡意本機程式仍可能讀取目前程序記憶體、使用者憑證或剪貼簿。
- 公共電腦不應保存永久 API Key；使用完畢後應從 GUI 清除。
- 企業環境應使用受限 API project、支出上限、最小權限與端點代理。
