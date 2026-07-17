# Implementation TODOs

## P0 — Release acceptance

- [x] 在 Windows 10 22H2 執行 fmt、clippy、tests 與 release build（32 tests passed）。
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

- [ ] 加入 tray-icon 與「隱藏到系統匣」。
- [ ] 實作啟動時自動執行。
- [ ] 加入 Windows Credential Manager。
- [ ] 改善中文標點與繁簡轉換。
- [ ] 加入 API rate-limit backoff 與可取消工作。
- [ ] 加入 MSI／NSIS installer、code signing、SBOM 與自動更新。

## P2 — Post-v1

- [ ] xAI WebSocket 即時字幕。
- [ ] OpenAI Realtime input transcription。
- [ ] 自動 VAD、連續聽寫、句尾 Smart Turn。

P2 與 v1 batch-transcription 發布範圍分離，不納入本次完成聲明。
