# Implementation TODOs

## P0 — Release acceptance

- [x] 在 Windows 10 22H2 執行 fmt、clippy、tests 與 release build（目前 98 tests passed）。
- [x] 確認目前鎖定的 rdev/enigo API 可於 Windows MSVC stable 編譯。
- [x] 以離線 mock request 測試驗證 xAI multipart `file` 欄位位於最後。
- [x] 驗證 API Key 不進 config serialization、Debug 或 provider error body。
- [x] 驗證無效 HWND、焦點競態、history 失敗與 clipboard fallback 失敗不會被靜默忽略。
- [x] 驗證 GUI 錄音不會以 SpeakType 自身 HWND 為目標，且 hotkey listener 失敗可由 UI 觀察。
- [ ] 在 Windows 11 執行完整 gate 與啟動 smoke。
- [ ] 用真實 OpenAI 與 xAI Key 完成 smoke test，不把 key 寫入 log。
- [ ] 測試提升權限與非提升權限視窗間的注入限制。
- [ ] 完成 Notepad、Chrome／Edge、VS Code 與 Office 注入矩陣。

## P1 — Product hardening

- [x] 加入 tray-icon 與「隱藏到系統匣」。
- [x] 實作啟動時自動執行。
- [x] 加入 Windows Credential Manager。
- [x] 改善中文標點與繁簡轉換。
- [x] 加入 API rate-limit backoff 與可取消工作。
- [x] 加入 NSIS installer、code signing、SBOM 與安全自動更新流程。

## P2 — Post-v1

- [x] xAI WebSocket 即時字幕。
- [x] OpenAI Realtime input transcription。
- [x] 自動 VAD、連續聽寫、句尾 Smart Turn。

Batch transcription 仍為預設安全路徑；P2 功能已實作但需真實 provider、實體麥克風與 Windows 10/11 互動 smoke 才能宣稱外部驗收完成。
