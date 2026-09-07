# 2026-09-07 維護紀錄

## 本次範圍

以主分支 `a59d9098c9f99e8b417eaf1851972269024196cd` 為基礎，聚焦文字注入安全、Windows 打包、GitHub 發行與使用說明。保留既有 Rust 架構、HTTP／WebSocket adapters、音訊 callback 邊界及 updater 簽章信任模型；沒有宣稱全面重寫或逐行驗證所有 GUI／provider 模組。

## 已處理問題

| 問題 | 處理 |
| --- | --- |
| 原 `focus_target(None)` 回傳成功，呼叫端可能對任意當前視窗送出 Ctrl+V | `inject_text` 在沒有目標時提早拒絕，並在存取剪貼簿後再次檢查焦點。 |
| 等待貼上後無條件還原先前文字剪貼簿 | 只有仍能讀到本次注入文字時才嘗試還原；不同內容或讀取失敗就保留目前剪貼簿。 |
| portable 必須複製 PDB，但 build-release 未輸出該 PDB | 正確搬移 Rust 產生的 PDB；符號改為選配，缺少符號不影響一般 ZIP。 |
| portable 會再次建置 app，可能覆蓋簽章／浪費時間 | 增加 `-SkipBuild` 並在 CI 重用已測試／簽署的 EXE。 |
| 兩套 workflow 重複 fmt、Clippy、tests、release build | 合併為單一 Windows CI and Release。 |
| CI 只有短期 artifact，沒有 GitHub Release | 新增最小權限的發布 job、版本驗證、draft 上傳與公開前檢查。 |
| 手動呼叫 manifest 腳本可為未簽章檔案產生 manifest | 腳本本身強制 Authenticode 與 signer pin 驗證。 |
| 缺少可核對下載檔案與來源的資訊 | 增加 SHA256SUMS、來源 commit／建置模式，並沿用 SBOM 與 hash-pinned NSIS provenance。 |
| 歷史預覽以 byte 截斷 UTF-8，混合中英文可能 panic | 改為最多 72 個 Unicode scalar values，補上中文、emoji 與邊界測試。 |
| NSIS 解除安裝指令的引號跳脫錯誤，編譯僅警告未失敗 | 修正一般／靜默解除安裝的路徑引號，加入靜態檢查與 `/WX`，讓 NSIS 警告阻擋發行。 |
| README 缺少一般使用者下載入口與清楚的更新條件 | 以下載、首次使用、隱私、排錯與維護操作重整，區分 GitHub 發行與 app 安裝更新。 |

## 驗證設計

新增 Rust 回歸測試涵蓋缺失目標、空文字、剪貼簿變更、讀取失敗及 Unicode 歷史預覽。PowerShell 行為測試使用隔離的暫存目錄與不可執行的假資料，驗證版本／來源政策、checksum 拒絕路徑、未簽章 manifest 防護、PDB 搬移、ZIP 必要內容及打包失敗時保留舊 ZIP；Release draft 的查找另覆蓋分頁、空集合與重複草稿。

`scripts/check.ps1` 是一致的品質入口；自動測試與建置結果以交付 commit 的 GitHub Actions 紀錄為準。此次編輯環境沒有 Rust 或 PowerShell，因此不把本機文字／YAML 檢查宣稱成 Windows 編譯通過。真實 API、麥克風、Windows 10／11 桌面互動及付費憑證更新仍需要外部實機驗收。

檢查完整 Windows 建置紀錄時，發現原本 NSIS 警告不會使 CI 失敗。因此在首次公開 Release 前取消該次主線建置，修正引號並改為警告即失敗後重新驗證；沒有用 CI 綠燈取代逐項檢視。

## 仍需留意

核對 `spawn_batch_transcription` 後，發現一般 `save_wav` 關閉不會停用歷史音訊保存。README／SECURITY 已明確揭露 Batch 的 `history/audio` 寫入，不再宣稱音訊完全不落地。本次保留歷史功能行為；仍需統一音訊保存 opt-in、錯誤工作殘留清理與 UI 提示。

剪貼簿比較與最後焦點檢查只是縮小競態窗口，不是原子交易，也不構成已收到貼上的確認。GUI 中的固定等待、HWND 重用、長時間歷史資料管理與 provider 真實環境行為仍可持續改善，不宜一次進行高風險的大範圍 UI 重構。

倉庫已追蹤 `.opencode` 的本機狀態／資料庫。此次加入忽略規則防止新增類似檔案，但**沒有擅自刪除既有記憶、改寫 Git 歷史或聲稱清除既有敏感資料**；是否需要清理，應先檢查內容與備份需求。

過往 `REVIEW_SUGGESTIONS.md` 是歷史檢視，不是現在每項功能狀態的權威來源；其中「尚無 CI」等敘述已不適用。使用目前程式碼、此紀錄與最新 Actions 結果判斷。
