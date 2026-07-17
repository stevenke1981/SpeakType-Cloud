# SpeakType Cloud — Product Specification

## 1. 產品目標

建立一個 Windows 桌面語音輸入法：使用者在任意文字輸入視窗按住全域快捷鍵說話，放開後由 OpenAI 或 xAI 辨識，再把文字注入原本使用中的視窗。

## 2. 必要功能

- 全域 Push-to-Talk 與切換錄音兩種模式。
- 記住錄音開始時的前景視窗。
- 麥克風選擇、增益、最短／最長錄音限制。
- 音訊正規化、轉 mono、重採樣到 16 kHz、輸出 PCM16 WAV。
- OpenAI `/v1/audio/transcriptions` adapter。
- xAI `/v1/stt` adapter，file 欄位最後加入 multipart。
- API Key 僅由環境變數載入。
- API timeout、HTTP 錯誤分類、空結果防護。
- 文字清理、剪貼簿注入、貼上失敗時保留文字。
- JSONL 歷史紀錄；WAV 保存預設關閉。
- 可直接用 Cargo 建置的 Windows Rust 專案。

## 3. 非目標（v1）

- 不提供本機 Whisper fallback。
- 不提供 OpenAI/xAI 即時 WebSocket streaming。
- 不攔截或替代 Windows IME 核心。
- 不上傳背景聲音；只有使用者主動錄音區段會送出。
- 不在設定檔保存明文 API Key。

## 4. 驗收條件

- 在 Notepad、瀏覽器文字框、VS Code 中均可完成 PTT → 辨識 → 貼上。
- 關閉自動注入時，結果只複製到剪貼簿。
- API Key 缺少、401、429、timeout、空回應都顯示可理解錯誤，且不遺失最後一次辨識文字。
- 開啟還原剪貼簿後，貼上完成約 120 ms 後恢復先前文字剪貼簿。
- 設定檔與日誌不得出現 API Key。
