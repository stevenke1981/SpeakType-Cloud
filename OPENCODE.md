# OpenCode / Codex Execution Prompt

請依序執行：

1. 閱讀 `AGENTS.md`、`SPEC.md`、`ARCHITECTURE.md`、`SECURITY.md`、`TODOS.md`。
2. 在 Windows 10/11 執行 `powershell -File scripts/check.ps1`。
3. 修正所有 compile、clippy 與 test 問題；不得降低 lint 或刪除測試掩蓋問題。
4. 建立 mock HTTP server 測試 OpenAI/xAI multipart request 與 error mapping。
5. 實機驗證 Notepad、Chrome、VS Code 的焦點視窗注入。
6. 每完成一項就在 `TODOS.md` 勾選，並把測試證據寫入 `FINAL.md`。
7. 除了刪除檔案、force push、修改 secrets 外，不要停下來詢問；遇到一般問題自行修復並繼續。
