use eframe::egui;
use std::path::PathBuf;

const CJK_FONT_KEY: &str = "speaktype-cjk";

pub fn install_cjk_font(ctx: &egui::Context) -> Result<PathBuf, String> {
    let candidates = cjk_font_candidates();

    for path in &candidates {
        if let Ok(bytes) = std::fs::read(path) {
            ctx.set_fonts(font_definitions_with_cjk(bytes));
            return Ok(path.clone());
        }
    }

    let checked_paths = candidates
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(format!(
        "找不到可用的中文字型，介面中文可能無法顯示。已檢查：{checked_paths}"
    ))
}

fn font_definitions_with_cjk(bytes: Vec<u8>) -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        CJK_FONT_KEY.to_owned(),
        egui::FontData::from_owned(bytes),
    );

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push(CJK_FONT_KEY.to_owned());
    }

    fonts
}

#[cfg(target_os = "windows")]
fn cjk_font_candidates() -> Vec<PathBuf> {
    let font_dir = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts");

    [
        "msjh.ttc",
        "msyh.ttc",
        "mingliu.ttc",
        "simsun.ttc",
        "meiryo.ttc",
        "malgun.ttf",
        "NotoSansCJK-Regular.ttc",
    ]
    .iter()
    .map(|file_name| font_dir.join(file_name))
    .collect()
}

#[cfg(target_os = "macos")]
fn cjk_font_candidates() -> Vec<PathBuf> {
    [
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn cjk_font_candidates() -> Vec<PathBuf> {
    [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-TC-Regular.otf",
        "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
        "/usr/share/fonts/truetype/arphic/uming.ttc",
    ]
    .iter()
    .map(PathBuf::from)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_font_is_added_as_fallback_for_both_font_families() {
        let fonts = font_definitions_with_cjk(vec![0, 1, 2]);

        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let fallback = fonts
                .families
                .get(&family)
                .and_then(|font_names| font_names.last())
                .map(String::as_str);
            assert_eq!(fallback, Some(CJK_FONT_KEY));
        }
    }
}
