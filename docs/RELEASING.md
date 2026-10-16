# 發行與維護指南

## 流程

```text
push／PR／手動執行
  → 驗證來源與版本
  → PowerShell 靜態＋行為測試、fmt、Clippy、Rust tests、release build
  → 重用執行檔 →（正式來源且配置憑證時）簽署 app
  → 建置 Inno Setup installer →（有配置時）簽署 installer
  →（簽章有效時）產生 updater manifest
  → portable ZIP、SBOM、build-info、SHA256SUMS
  → Actions artifact
  → 僅預設主分支或匹配 tag：獨立 publish job
  → 驗證 artifact → 建立／恢復 draft → 驗證遠端 assets → 公開 Release
```

建置 job 只有 `contents: read`；發布 job 才有 `contents: write`。Actions 固定完整 commit SHA，checkout 不保留 Git 認證。Pull request 使用一般 `pull_request` 事件，不使用 `pull_request_target` 執行貢獻者程式碼，也不使用簽章秘密。簽章材料在 `always()` 步驟清理。

## 版本規則

唯一版本來源是 `Cargo.toml` 的 `[package].version`。第一次將尚未發布的版本推送主分支，workflow 即自動建立 `v<版本>` tag／Release；不需要另設 PAT，也不依賴建立 tag 後再觸發另一個 workflow。

相同版本已公開時，發布步驟會記錄「未修改」後結束。**不會自動增加版號、不會以新二進位覆蓋舊 Release，也不會 force-move tag。** 程式變更要發行下一版時：

```powershell
# 先修改 Cargo.toml 的 package.version，例如 0.1.0 → 0.1.1。
cargo update --workspace
# 檢查 Cargo.lock 的變更符合預期，再執行 gate。
pwsh -NoProfile -File .\scripts\check.ps1
git diff -- Cargo.toml Cargo.lock
git add Cargo.toml Cargo.lock
git commit -m "chore: release 0.1.1"
git push origin main
```

使用實際預設分支；程式會從 repository metadata 判斷 `main`、`master` 或其他預設名稱。也可在 Cargo 版本匹配的 commit 手動建立 `v*` tag。Prerelease 版本如 `0.2.0-rc.1` 會標記為 prerelease，不成為 latest。

Actions 的 `Run workflow` 可重跑建置；選擇其他開發分支時不會發布。未公開的 draft 只允許相同來源 SHA 重跑續傳。HTTP 401／403、網路失敗不會被誤判成「Release 不存在」。

## 未簽章與簽章模式

| 模式 | 發布下載檔 | `update-manifest.json` | 程式內自動安裝 |
| --- | --- | --- | --- |
| 未設定 signer pin | 可，明確標示 UNSIGNED | 不產生 | 停用，改用手動下載 |
| 已設定 pin 且憑證／密碼有效 | 可，app 與 installer 均簽章 | 驗證簽章後才產生 | hash、簽章、pin 與使用者確認皆通過才允許 |
| pin 格式錯誤或缺少簽章秘密 | 失敗，不降級 | 不發布 | 不允許 |

在 repository 的 **Settings → Secrets and variables → Actions** 設定：

| 類型 | 名稱 | 內容 |
| --- | --- | --- |
| Variable | `UPDATE_SIGNER_CERT_SHA256` | Leaf signing certificate 的 DER SHA-256，64 個十六進位字元。不是 PFX 檔案雜湊。 |
| Secret | `WINDOWS_SIGNING_PFX_BASE64` | Code-signing PFX 的 Base64。 |
| Secret | `WINDOWS_SIGNING_PFX_PASSWORD` | PFX 密碼。 |

不要把 PFX、密碼或 Base64 貼進 issue、README 或 commit。憑證必須能通過 Windows Authenticode 信任驗證；自簽憑證不能直接視為所有使用者都信任。換憑證需要考慮舊版的 pin 遷移，不能單純移除驗證。

`new-update-manifest.ps1` 本身也會查驗 Authenticode 與 certificate pin，手動執行時亦不能為 unsigned installer 產生更新 manifest。雜湊針對簽署完成後的 bytes 計算，portable 也直接包裝已簽署 app，不重編譯覆蓋簽章。

若既有 latest 含 signed updater manifest，新 unsigned 發行版不會取代它為 latest；較舊版本也不會把新版 latest 降回舊版。新的 signed stable Release 才能安全延續固定的 updater 下載端點。

## 產物與查驗

每次 CI 提供 `SpeakTypeCloud-windows-x64-<commit>` artifact，保留 14 天。公開 Release 是給一般使用者下載的發行介面，與短期 Actions artifact 不同。

發布檔案只有 installer、portable ZIP、SBOM、建置工具 provenance、`build-info.json`、`SHA256SUMS.txt`，以及簽章模式下的 updater manifest。**不將設定、錄音、歷史、PFX 或整個 workspace 打包。**

建置資訊記錄版本、來源 SHA、平台、Rust 版本、簽章模式與 workflow run。發布 job 重新驗證所有 checksum、檔名 allowlist、來源 SHA 與版本；先上傳 draft，確認檔案名稱、大小與 GitHub 回傳的 digest（如有）後才公開。雜湊清單缺檔、多檔、重複名稱、路徑穿越與內容竄改都會被拒絕。

除錯符號預設不放入一般下載包。需要時在本機執行：

```powershell
pwsh -NoProfile -File .\scripts\build-release.ps1
pwsh -NoProfile -File .\scripts\package-portable.ps1 -SkipBuild -IncludeSymbols
```

## 實機驗收

使用乾淨 Windows 10／11 測試：安裝／啟動、麥克風與全域快捷鍵、供應商真實辨識、取消與逾時、記事本／瀏覽器／VS Code／Office 貼上、焦點快速切換、剪貼簿被另一程式改寫、系統匣、自啟及解除安裝。簽章模式另外驗證「檢查更新 → 下載 → hash／Authenticode／pin → 確認啟動安裝精靈」。

不可把離線 mocks、打包 fixture 或 Windows runner 測試當成真實 API／實體裝置驗收。未簽章版本不要透過關閉安全檢查來測試自動更新。

## 官方參考

- [GitHub：GITHUB_TOKEN 與工作流程觸發](https://docs.github.com/en/actions/concepts/security/github_token)
- [GitHub CLI：建立 Release](https://cli.github.com/manual/gh_release_create)
- [GitHub CLI：編輯 Release](https://cli.github.com/manual/gh_release_edit)
- [GitHub：Immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases)
- [Cargo update](https://doc.rust-lang.org/cargo/commands/cargo-update.html)

## 0.2.0 Inno build and isolated acceptance

The audited compiler is Inno Setup 7.1.0 x64, SHA-256 `0362a383ed217d4c4239b5933866dd96d3eb2102737da92f80f6057a4b40df2f`, official Pyrsys B.V. signature. `scripts/install-inno-build-tool.ps1` verifies the pinned official download before a current-user tool installation. Pass `-IsccPath` or use the script-provided `SPEAKTYPE_ISCC`.

```powershell
pwsh -NoProfile -File scripts/install-inno-build-tool.ps1
pwsh -NoProfile -File scripts/build-installer.ps1 -SkipBuild -IsccPath '<verified ISCC.exe>'
pwsh -NoProfile -File scripts/check-runtime.ps1 -Executable dist/release/SpeakTypeCloud.exe
pwsh -NoProfile -File scripts/test-inno-install.ps1 -IsccPath '<verified ISCC.exe>' -EvidenceRoot '<owned workspace>'
```

Stable AppId `SpeakTypeCloud.Slint`, current-user installation, new default application directory, optional desktop shortcut, Start Menu, app mutex guard, no forced app termination. It checks both registry views for the legacy NSIS uninstall registration and remembered custom directory. The user must normally remove the old app first. The installer never evaluates registry commands, invokes the old uninstaller or recursively removes a directory. Cancellation before installation leaves the old app untouched. After the user removes the old app, a failed/cancelled new install leaves it uninstalled but preserves profile data for retry. Old and new installers must never own the same files.

The QA script creates a unique branded installer and registry namespace, synthetic legacy markers, four synthetic profile sentinels, and shortcuts with `--demo`. It verifies refusal, fresh install, hashes, licenses, shortcuts, re-run and normal uninstall. This tests real Inno execution, but synthetic legacy markers are not an actual NSIS-uninstaller upgrade test, and a fake credential file is not a live Credential Manager test. `-KeepInstalled` permits native UI acceptance before normal removal. Never publish fixture packages.

MSVC CRT is statically linked in `.cargo/config.toml`; runtime verification inspects normal and delayed PE imports, refusing non-system DLL dependencies. Windows 10/11 remains the OS prerequisite. Portable and installer packages include MIT, third-party notices and Slint Royalty-free 2.0 terms. Signing policy is unchanged.

Before publishing 0.2.0, freeze an immutable SHA, obtain independent review and matching CI, inspect the actual app/installer workflows, and record remaining NOT_RUN items. Do not merge conflicting branches, move existing tags, replace existing assets or infer live provider quality from mocks.
