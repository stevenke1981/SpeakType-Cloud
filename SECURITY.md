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

## 音訊與本機資料

- WAV 保存預設關閉。
- Batch 只有使用者主動開始的錄音區段會送往選定供應商。
- Realtime 與 Continuous 模式預設關閉；只有使用者明確啟動 session 後才將音訊送出，停止／取消後等待 worker 確認結束。
- 資料位置由 `src/paths.rs` 計算；錄音放在 data directory 的 `recordings`，歷史文字放在 `history/entries.jsonl`。
- portable ZIP 只是免安裝發行方式，設定／歷史仍使用 Windows 使用者目錄，Key 仍使用 Credential Manager。
- 解除安裝會保留使用者設定、歷史、錄音與 Credential Manager 項目，不應把解除安裝當成資料抹除。

## 更新供應鏈

- 更新器只接受本專案 GitHub Releases 的 HTTPS URL，限制回應大小並驗證 SHA-256。
- 自動啟動 installer 前必須同時通過 Authenticode `Valid` 與建置時固定的 leaf certificate DER SHA-256；缺少 pin 時更新器停用。
- PR／開發分支只產生 unsigned CI artifacts，不使用簽章秘密，不發布 Release。
- 預設主分支或匹配版本 tag 可自動發布：未配置 pin 時，發布清楚標示 unsigned 的手動下載版；配置 pin 後必須有有效簽章材料，不允許失敗後降級為 unsigned。
- 只有完成簽章與 pin 驗證的 installer 才能產生 updater manifest。`new-update-manifest.ps1` 手動呼叫也會執行此檢查。
- 同一版號已公開的 Release 不覆蓋，既有 tag 不重指；失敗 draft 只允許相同來源 commit 重跑續傳。
- 下載包附有 SHA256SUMS、SBOM、來源 commit 與建置工具 provenance。checksum 不是獨立的發行者身分保證。
- Build job 為 read-only，publish job 才有 `contents: write`；先檢查完整產物再公開 draft。
- 更新不使用 silent install，必須由使用者明確確認啟動安裝精靈。
- `.pfx`、`.p12` 與本機 agent runtime state 不應提交。`.gitignore` 不會回溯移除已追蹤檔案，也不代表 Git 歷史已被清理。

## 視窗與剪貼簿

- 只保存 HWND 數值於記憶體，不保存視窗標題或內容。
- 缺少目標視窗時拒絕自動貼上；切回目標後，以及存取剪貼簿後、送出 Ctrl+V 前，都會檢查前景視窗。
- 可選擇貼上後還原先前「文字」剪貼簿；圖片或其他複合格式不會完整還原。
- 只有剪貼簿仍可讀到本次注入文字時才嘗試還原；已改變或無法讀取時跳過，避免已偵測到的新內容被覆蓋。
- 焦點與剪貼簿檢查仍是 best-effort，不是跨程式原子交易，也不證明目標程式已完成讀取。
- 高權限視窗可能拒絕低權限程式注入，失敗時保留可手動複製的辨識結果。

## 威脅模型

- 惡意本機程式仍可能讀取目前程序記憶體、使用者憑證或剪貼簿。
- 公共電腦不應保存永久 API Key；使用完畢後應從 GUI 清除。
- 企業環境應使用受限 API project、支出上限、最小權限與端點代理。
- 回報安全問題時不要公開真實 Key、錄音、私人辨識文字或簽章秘密。

發行設定與實機驗收請見 [docs/RELEASING.md](docs/RELEASING.md)。
