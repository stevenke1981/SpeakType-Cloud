# Development Plan

## Phase 1 — 可用 MVP（本包已提供）

- [x] Rust/egui Windows app shell
- [x] cpal microphone capture
- [x] global PTT hotkey
- [x] focused-window capture and clipboard injection
- [x] OpenAI batch transcription adapter
- [x] xAI batch transcription adapter
- [x] environment-variable API key policy
- [x] config, history, optional recording retention
- [x] unit tests and Windows CI definition

## Phase 2 — 強化

- [ ] 系統匣、開機啟動、隱藏主視窗
- [ ] Windows Credential Manager integration
- [ ] xAI streaming STT adapter
- [ ] OpenAI Realtime transcription adapter
- [ ] VAD 自動分段與連續聽寫模式
- [ ] 自訂字典、語音指令、情境提示詞
- [ ] 安裝程式與自動更新

## Phase 3 — 發布

- [ ] Notepad/Chrome/Edge/VS Code/Office 相容性矩陣
- [ ] 錯誤遙測（opt-in、無音訊與無 key）
- [ ] 簽章、SBOM、依賴掃描
- [ ] 壓力測試與長時間記憶體測試
