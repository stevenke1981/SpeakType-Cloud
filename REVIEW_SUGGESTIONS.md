# SpeakType Cloud — 專案檢視與優化建議

> 檢視日期：2026-07-27
> 範圍：全部 `src/` 模組、`Cargo.toml`、`scripts/`、文件
> 目前狀態：98 tests passed，P0/P1/P2 功能已實作，尚缺 Windows 11 smoke 與真實 provider 驗收

---

## 一、現有優點（值得保留）

| 面向 | 說明 |
|------|------|
| API Key 安全 | Credential Manager 儲存、config 只存 env name、error body redact、Debug 不含 key |
| 錯誤處理 | `AppError` 分類清楚、retry with exponential backoff、cancellation token 貫穿 batch/realtime |
| 測試覆蓋 | 98+ unit tests，涵蓋 edge case（stale job、rollback、boundary、redaction） |
| 架構邊界 | providers/ 不依賴 egui/hotkey/injector；cpal callback 不做 I/O |
| 更新安全 | SHA-256 + Authenticode + signer cert pin + HTTPS-only + 大小限制 + 三段確認 |
| 注入防護 | HWND 驗證、self-window 排除、焦點競態檢查、失敗保留文字 |

---

## 二、建議新增功能

### 2.1 歷史紀錄檢視器（UI）

**現況：** JSONL 歷史只寫不讀，使用者無法在 GUI 查詢、搜尋或匯出。

**建議：**
- 在 UI 加入「歷史紀錄」折疊區或獨立視窗
- 支援關鍵字搜尋、日期篩選
- 提供「複製」、「再次貼上」、「匯出 CSV」操作
- 加入分頁或虛擬捲動避免大量記錄拖慢 UI

### 2.2 錄音音量即時視覺回饋

**現況：** 錄音中只顯示「🔴 錄音中」文字，使用者無法確認麥克風是否正常收音。

**建議：**
- 在 cpal callback 計算 RMS 並透過 `Arc<AtomicU32>` 或類似機制傳回 UI
- 顯示簡易音量條（level meter），讓使用者即時看到收音狀態
- 連續聽寫模式下也可顯示 VAD 判定（speech/silence）狀態

### 2.3 自訂詞典與語音命令 GUI 編輯

**現況：** `dictionary` 和 `voice_commands` 只能手動編輯 `config.toml`。

**建議：**
- 在「API 與語言」設定區加入詞典管理表格（source → replacement）
- 加入語音命令管理（phrase + action 下拉選單）
- 儲存時寫回 config 並 validate

### 2.4 單實例保護

**現況：** 可同時啟動多個实例，造成 hotkey 衝突、剪貼簿競態。

**建議：**
- 使用 Windows Mutex（`CreateMutexW`）或 named pipe 做單實例檢查
- 第二個实例啟動時顯示訊息並 focus 既有視窗，或直接退出

### 2.5 設定匯入／匯出

**現況：** 沒有備份/還原設定的方式。

**建議：**
- 提供「匯出設定」按鈕，將 config.toml 複製到使用者指定位置
- 提供「匯入設定」按鈕，讀取並 validate 後覆蓋

### 2.6 使用統計

**現況：** 沒有使用量資訊。

**建議：**
- 記錄每次辨識的 provider、音訊長度、耗時、成功/失敗
- 在 UI 顯示簡單統計（今日/本週/累計）
- 不上傳任何資料，純本機

### 2.7 深色主題

**現況：** 只有淺色主題（`theme.rs`）。

**建議：**
- 加入深色主題色盤，跟隨 Windows 設定或手動切換
- egui 的 `Visuals::dark()` 作為基底再微調

### 2.8 HTTP Proxy 設定

**現況：** 企業環境可能需要 proxy，目前 reqwest 只吃環境變數。

**建議：**
- 在設定中加入 proxy URL 欄位（選配）
- 傳入 `reqwest::Client::builder().proxy()`

### 2.9 版本號顯示於主畫面

**現況：** 版本號只在更新視窗顯示。

**建議：**
- 在主畫面標題旁或底部顯示 `v{CARGO_PKG_VERSION}`

---

## 三、建議改善項目

### 3.1 拆分 `app.rs`（1272 行）

**問題：** 單一檔案包含狀態機、錄音控制、transcription polling、realtime polling、UI 渲染、設定管理。

**建議拆分：**
```
src/
  app/
    mod.rs          — SpeakTypeCloudApp struct + eframe::App impl
    recording.rs    — start/stop/toggle recording 邏輯
    polling.rs      — poll_hotkey / poll_transcription / poll_realtime
    ui.rs           — CentralPanel 渲染
    settings.rs     — save_settings / apply_settings_transaction
```

### 3.2 拆分 `shell.rs`（733 行）

**建議拆分：**
```
src/
  shell/
    mod.rs          — AppleShell + eframe::App
    tray.rs         — SystemTray
    api_key_ui.rs   — API 金鑰視窗
    update_ui.rs    — 更新視窗
```

### 3.3 引入結構化日誌

**現況：** 只有 `eprintln!` 輸出字型錯誤。生產環境無法追蹤問題。

**建議：**
- 加入 `tracing` + `tracing-subscriber`（或 `log` + `env_logger`）
- 將重要事件（錄音開始/停止、provider 呼叫、注入結果、錯誤）記錄為 structured log
- 日誌寫入 `%LOCALAPPDATA%\SpeakType\SpeakTypeCloud\data\logs\`
- **絕對不記錄 API Key、音訊內容、辨識文字**

### 3.4 消除重複的 `wide()` 函式

**現況：** `secrets.rs` 和 `startup.rs` 各自定義了相同的 `wide()` 函式。

**建議：** 移到共用的 `src/win_util.rs` 或 `src/paths.rs`。

### 3.5 改善 `realtime_settings_fingerprint`

**現況：** 用 `format!("{:?}|{:?}|...", ...)` 產生指紋，依賴 Debug 格式，脆弱。

**建議：**
- 對各欄位計算 `std::hash::Hash` 並比較
- 或定義 `PartialEq` 在 `RealtimeConfig` 上，直接比較 struct

### 3.6 連續重繪最佳化

**現況：** `ctx.request_repaint_after(Duration::from_millis(35))` 無條件執行，即使閒置也以 ~28 FPS 重繪。

**建議：**
- 只在錄音中、辨識中、realtime 進行中時使用 35ms 重繪
- 閒置時改用 `request_repaint_after(Duration::from_millis(500))` 或完全被動
- 減少 CPU/GPU 閒置功耗

### 3.7 CaptureBuffer 記憶體用量

**現況：** 預配置 `sample_rate × channels × 120s`。48 kHz stereo = ~46 MB，即使只錄 3 秒。

**建議：**
- 改用分段配置（chunked ring buffer）或 `Vec` 動態增長 + 上限
- 或將 120s 上限改為可配置，預設 60s

### 3.8 音訊重採樣品質

**現況：** `resample_linear` 是線性插值，對 48→16 kHz 降採樣會產生 aliasing。

**建議：**
- 短期：加入簡單的低通濾波器（one-pole 或 biquad）在降採樣前
- 長期：使用 `rubato` crate 做 polyphase resampling

### 3.9 剪貼簿還原時機

**現況：** Ctrl+V 後固定 `sleep(120ms)` 再還原。慢應用程式可能來不及讀取。

**建議：**
- 改用 Windows clipboard viewer chain 或 `AddClipboardFormatListener` 監聽目標視窗讀取完成
- 或提供可配置的延遲（100–500ms）

### 3.10 Updater Authenticode 驗證

**現況：** 透過 `powershell.exe` 呼叫 `Get-AuthenticodeSignature`。啟動慢（~1–2s）、可被 PATH 劫持。

**建議：**
- 改用 Windows API `WinVerifyTrust` + `CryptQueryObject` 直接驗證
- 需加入 `windows-sys` features：`Win32_Security_Cryptography_Catalog`、`Win32_Security_WinTrust`
- 消除對 PowerShell 的依賴

### 3.11 準備 Rust 2024 edition

**現況：** `std::env::set_var` / `remove_var` 在 Rust 2024 為 unsafe。目前 edition 2021。

**建議：**
- 在 `secrets.rs` 中將 `set_var`/`remove_var` 包在 `unsafe {}` 中並加 safety comment
- 或改用 `windows-sys` 的 `SetEnvironmentVariableW` / `GetEnvironmentVariableW`
- 逐步準備 edition 遷移

### 3.12 Mutex poison 處理

**現況：** `Recorder::stop()` 對 poisoned mutex 使用 `poisoned.into_inner()` 靜默恢復。

**建議：**
- 至少用 `eprintln!` 或日誌記錄 warning
- 評估是否應視為錯誤而非靜默繼續

---

## 四、建議移除或簡化

### 4.1 `append_space` 選項

**現況：** 在 CJK 輸入場景下極少使用，增加 postprocess 與 UI 複雜度。

**建議：** 移除或標記為 deprecated，預設 `false` 且不在 UI 顯示。

### 4.2 `copy_only_on_injection_failure` 選項

**現況：** 預設 `true`，實際上注入失敗時保留文字已是核心行為。

**建議：** 簡化為「注入失敗一律 fallback 到剪貼簿」，移除此設定項。

### 4.3 `OpenAiTranscriptionDelay` 五級選項

**現況：** minimal/low/medium/high/xhigh 五級，使用者難以理解差異。

**建議：** 簡化為「快速 / 標準 / 高品質」三級，內部映射到 API 值。

---

## 五、測試與 CI 建議

### 5.1 加入整合測試

- 用 mock HTTP server（已有 `serve_once`）擴展為完整 batch pipeline 測試
- 用 mock WebSocket server（已有）擴展為完整 realtime pipeline 測試
- 測試 config 載入 → hydrate → transcribe → inject 全流程

### 5.2 加入 UI 測試

- 使用 `egui_kittest` 或 `eframe` 的 test harness
- 測試按鈕啟用/停用邏輯、設定儲存 rollback、API key 視窗互動

### 5.3 加入 CI pipeline

**現況：** 沒有 `.github/workflows/`。

**建議：**
```yaml
# .github/workflows/ci.yml
- cargo fmt --check
- cargo clippy --all-targets -- -D warnings
- cargo test --all-targets
- cargo build --release
```
- 在 Windows runner 上執行（專案為 Windows-first）
- 加入 `scripts/check.ps1` 作為單一入口

### 5.4 Config 遷移測試

- 當 config schema 變更時，確保舊版 config.toml 仍可正確載入
- 加入 golden file 測試：固定舊版 config 字串 → parse → validate

---

## 六、安全性建議

| 項目 | 現況 | 建議 |
|------|------|------|
| Authenticode 驗證 | PowerShell 外部呼叫 | 改用 `WinVerifyTrust` API |
| 憑證撤銷檢查 | 無 | 加入 CRL/OCSP 檢查（`WinVerifyTrust` 預設會做） |
| 記憶體中 API Key | `String` 常駐 | 使用 `zeroize` crate 在 drop 時清零 |
| 日誌 | 無 | 加入後需確保不記錄 key/text/audio |
| 單實例 | 無 | 防止多实例競態 |

---

## 七、優先級排序

### 高優先（影響可靠性/安全性）
1. 單實例保護（3.4 → 2.4）
2. 結構化日誌（3.3）
3. Authenticode 改用 WinVerifyTrust（3.10）
4. 連續重繪最佳化（3.6）
5. CI pipeline（5.3）

### 中優先（改善開發/維護效率）
6. 拆分 app.rs / shell.rs（3.1, 3.2）
7. 消除重複 `wide()`（3.4）
8. 改善 realtime fingerprint（3.5）
9. 準備 Rust 2024（3.11）
10. 整合測試（5.1）

### 低優先（使用者體驗增強）
11. 歷史紀錄檢視器（2.1）
12. 音量即時回饋（2.2）
13. 詞典/語音命令 GUI（2.3）
14. 深色主題（2.7）
15. 設定匯入匯出（2.5）
16. 使用統計（2.6）
17. Proxy 設定（2.8）

---

## 八、不建議變更

- **Batch PTT 作為預設模式**：安全、簡單、可靠，不應改變預設。
- **Clipboard + Ctrl+V 注入方式**：對中文和大量文字最可靠，不應改為逐字模擬。
- **JSONL 歷史格式**：簡單、append-only、不易損壞，優於 SQLite。
- **Credential Manager 儲存 API Key**：Windows 原生、使用者層級隔離，優於自訂加密檔案。
- **providers/ 不依賴 UI**：架構邊界正確，不應破壞。
