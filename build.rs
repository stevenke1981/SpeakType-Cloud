fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set("FileDescription", "SpeakType Cloud");
        res.set("ProductName", "SpeakType Cloud");
        res.set("LegalCopyright", "MIT License");

        // Embed DPI awareness manifest for sharp rendering on HiDPI displays.
        // Declares PerMonitorV2 so the window is re-rendered when DPI changes
        // (e.g. moved between monitors with different scaling factors).
        res.set_manifest(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettingsProtocol">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>"#,
        );

        if let Err(error) = res.compile() {
            println!("cargo:warning=Windows resource compile skipped: {error}");
        }
    }
}
