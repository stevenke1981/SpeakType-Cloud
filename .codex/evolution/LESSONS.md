# Reusable engineering lessons

- In PowerShell, `$ErrorActionPreference = "Stop"` does not make a failing native executable terminate the script. Check `$LASTEXITCODE` immediately after each required native command so later successful commands cannot create a false-green gate.
- Rebuild release staging from a verified empty directory and reject prohibited runtime data before compression; successful copying alone does not prove a clean package.
- Clipboard injection must preserve the original external HWND, exclude the application's own windows, and revalidate the foreground target immediately before sending paste input.
