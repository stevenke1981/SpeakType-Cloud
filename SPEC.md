# SpeakType Cloud — Product Specification

## 1. 產品目標

建立一個 Windows 桌面語音輸入法：使用者在任意文字輸入視窗按住全域快捷鍵說話，放開後由 OpenAI、xAI 或 OpenRouter 辨識，再把文字注入原本使用中的視窗。

## 2. 必要功能

- 全域 Push-to-Talk 與切換錄音兩種模式。
- 記住錄音開始時的前景視窗。
- 麥克風選擇、增益、最短／最長錄音限制。
- 音訊正規化、轉 mono、重採樣到 16 kHz、輸出 PCM16 WAV。
- OpenAI `/v1/audio/transcriptions` adapter。
- xAI `/v1/stt` adapter，file 欄位最後加入 multipart。
- OpenRouter `/v1/audio/transcriptions` JSON base64 adapter（僅 Batch）。
- OpenAI Realtime transcription 與 xAI WebSocket streaming adapters。
- Batch PTT 預設、Realtime PTT 與 Continuous Dictation 明確 opt-in。
- 本機 VAD、xAI Smart Turn、partial 字幕與 ordered final utterances。
- API Key 可由環境變數或 Windows Credential Manager 載入，不進設定檔。
- API timeout、HTTP 錯誤分類、空結果防護。
- 文字清理、剪貼簿注入、貼上失敗時保留文字。
- JSONL 歷史紀錄；WAV 保存預設關閉。
- 可直接用 Cargo 建置的 Windows Rust 專案。
- 系統匣、登入自啟、NSIS 發佈模板、SBOM、簽署與安全更新流程。

## 3. 非目標

- 不提供本機 Whisper fallback。
- 不攔截或替代 Windows IME 核心。
- 不上傳背景聲音；只有使用者主動錄音區段會送出。
- 不在設定檔保存明文 API Key。

## 4. 驗收條件

- 在 Notepad、瀏覽器文字框、VS Code 中均可完成 PTT → 辨識 → 貼上。
- 關閉自動注入時，結果只複製到剪貼簿。
- API Key 缺少、401、429、timeout、空回應都顯示可理解錯誤，且不遺失最後一次辨識文字。
- 開啟還原剪貼簿後，貼上完成約 120 ms 後恢復先前文字剪貼簿。
- 設定檔與日誌不得出現 API Key。
- Realtime session 取消後不得注入 stale result；Continuous utterances 必須按 commit 順序輸出。
- 更新 installer 必須通過 hash、有效 Authenticode 與固定 signer identity，且不得靜默安裝。
