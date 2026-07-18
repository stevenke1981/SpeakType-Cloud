# UI Design

## 主畫面

- 頂部：產品名稱、操作說明。
- 狀態列：就緒／錄音／辨識／已貼上／錯誤。
- 主操作：開始／停止／取消、再次貼上、複製文字；Realtime 顯示 partial 字幕。
- 最近文字：可編輯 multiline 區域。
- 摺疊設定：API 與語言、Batch／Realtime／Continuous 模式、VAD／Smart Turn、錄音、文字處理與輸出。

## 狀態色彩

- 錄音：紅色圓點。
- 辨識：雲端符號與文字。
- 錯誤：紅色訊息，不使用阻塞式 modal。

## 可用性

- 預設 PTT：`Ctrl+Shift+Space`。
- API Key 使用獨立、預設遮蔽的浮動面板輸入；儲存後立即清空欄位，且不得寫入 `config.toml` 或錯誤輸出。
- 中文主介面，模型與 API 名稱保留英文。
- 系統匣提供顯示與明確退出；關閉視窗時只有在 tray 可用時才隱藏，避免無法退出。
- 更新採「檢查 → 下載驗證 → 啟動安裝精靈」三段確認；缺少簽署 trust root 時只顯示手動 Releases。
