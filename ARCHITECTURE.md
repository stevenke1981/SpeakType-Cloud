# Architecture

```text
GlobalHotkey ─┐
              ├─> App state ─> Recorder(cpal) ─> WAV encoder
GUI buttons ──┘                                  │
                                                 v
                              Provider abstraction (blocking worker thread)
                               ├─ OpenAI /v1/audio/transcriptions
                               └─ xAI /v1/stt
                                                 │
                                                 v
                               postprocess -> history -> WindowTarget restore
                                                 │
                                                 v
                                      clipboard + Ctrl+V injection
```

## 執行緒界線

- egui 主執行緒：UI、錄音開始／停止、狀態機。
- rdev listener thread：只產生 Pressed/Released 事件，不直接操作 UI 或錄音。
- transcription worker：WAV 編碼後呼叫 blocking HTTP API，透過 channel 回傳。
- cpal callback：只把取樣寫入 Mutex buffer，不做 I/O、HTTP 或 UI。

## 供應商抽象

`SpeechToTextProvider` 只有一個 `transcribe()` 介面。App 不知道端點差異。OpenAI adapter 負責 model/language/prompt；xAI adapter 負責 format/language/keyterm 與 multipart 欄位順序。

## 文字注入

錄音開始時使用 `GetForegroundWindow` 保存 HWND。辨識完成後驗證 `IsWindow`，呼叫 `SetForegroundWindow`，再將文字暫存到剪貼簿並送出 Ctrl+V。這比逐字模擬鍵盤更適合中文與大量文字。
