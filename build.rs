fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set("FileDescription", "SpeakType Cloud");
        res.set("ProductName", "SpeakType Cloud");
        res.set("LegalCopyright", "MIT License");
        if let Err(error) = res.compile() {
            println!("cargo:warning=Windows resource compile skipped: {error}");
        }
    }
}
