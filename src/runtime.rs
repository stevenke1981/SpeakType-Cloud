//! Parse launch policy before creating any OS services. Demo is a capability
//! boundary, not merely a UI badge; it must never load the live user profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeMode {
    Live,
    Demo,
}

impl RuntimeMode {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let args: Vec<_> = args.into_iter().collect();
        match args.as_slice() {
            [] => Ok(Self::Live),
            [arg] if arg == "--demo" => Ok(Self::Demo),
            _ => Err("用法：SpeakTypeCloud.exe [--demo]；示範模式不讀取金鑰、設定或歷史。".into()),
        }
    }

    pub fn is_demo(self) -> bool {
        self == Self::Demo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_is_explicit_and_unknown_flags_fail_closed() {
        assert_eq!(RuntimeMode::parse([]), Ok(RuntimeMode::Live));
        assert_eq!(RuntimeMode::parse(["--demo".into()]), Ok(RuntimeMode::Demo));
        assert!(RuntimeMode::parse(["--demo".into(), "--live".into()]).is_err());
        assert!(RuntimeMode::parse(["--demoo".into()]).is_err());
    }
}
