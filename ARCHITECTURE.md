# Architecture

```text
GlobalHotkey / GUI
        │
        v
    App state ───────────────┬── Batch Recorder -> WAV -> async HTTP worker
        │                    │                    ├─ OpenAI /v1/audio/transcriptions
        │                    │                    ├─ xAI /v1/stt
        │                    │                    └─ OpenRouter /v1/audio/transcriptions (JSON base64)
        │                    │
        │                    └── Live capture -> bounded channel -> realtime worker
        │                                         ├─ OpenAI WS /v1/realtime (24 kHz)
        │                                         └─ xAI WS /v1/stt (16 kHz)
        │                                         (OpenRouter: rejected at validation)
        │                                                  │
        │                                    local VAD / xAI Smart Turn
        v                                                  v
  partial UI                         final -> postprocess -> history
                                                        │
                                                        v
                                         WindowTarget -> clipboard + Ctrl+V
```

## 執行緒界線

- egui 主執行緒：UI、錄音開始／停止、狀態機。
- rdev listener thread：只產生 Pressed/Released 事件，不直接操作 UI 或錄音。
- batch transcription worker：WAV 編碼後以可取消的 async HTTP 呼叫供應商，透過每個 Job 專屬 channel 與 JobId 回傳。
- realtime worker：擁有 WebSocket、stateful resampler、固定 10 ms VAD frames 與 session cancellation；停止後送出 `Stopped` ack，UI 才允許下一個工作。
- cpal callback：只做 gain／mono、固定容量 capture ring 與 pooled bounded `try_send`；不等待鎖、不做 I/O、HTTP 或 UI，丟棄數量可觀察。

## 供應商抽象

Batch `SpeechToTextProvider` 與 realtime session abstraction 分離，兩者都不依賴 egui、hotkey 或 injector。OpenAI realtime 累積 delta 並依 commit/item 順序交付 final；xAI 只有 `speech_final=true` 才形成 utterance final，chunk-final 只鎖定 partial。

## P1 系統整合

- API Key：process environment 優先，其次 Windows Credential Manager；外部 `HKCU\Environment` 只讀取匯入、不刪除。
- 系統匣：tray 可用時 X 關閉改為隱藏；tray 失敗時保留正常退出。
- 登入自啟：app-owned `HKCU\...\Run` value，config／registry／runtime hotkey 使用可回滾 transaction。
- 更新器：固定 GitHub repo、大小限制、SHA-256、Authenticode 與 signer certificate pin，明確三段使用者確認。

## 文字注入

錄音開始時使用 `GetForegroundWindow` 保存 HWND。辨識完成後驗證 `IsWindow`，呼叫 `SetForegroundWindow`，再將文字暫存到剪貼簿並送出 Ctrl+V。這比逐字模擬鍵盤更適合中文與大量文字。
