use eframe::egui;

/// Apple-inspired design tokens
pub mod colors {
    use eframe::egui::Color32;

    // MARK: - Core palette
    pub const ACCENT_BLUE: Color32 = Color32::from_rgb(0, 122, 255);
    pub const ACCENT_BLUE_HOVER: Color32 = Color32::from_rgb(10, 132, 255);
    pub const ACCENT_BLUE_PRESSED: Color32 = Color32::from_rgb(0, 102, 204);

    // MARK: - Backgrounds (layered, glass-like)
    pub const BG_BASE: Color32 = Color32::from_rgb(245, 245, 247);
    pub const BG_CARD: Color32 = Color32::from_rgb(255, 255, 255);
    pub const BG_CARD_ALT: Color32 = Color32::from_rgb(250, 250, 252);

    // MARK: - Fill / stroke
    pub const FILL_SECONDARY: Color32 = Color32::from_rgb(238, 238, 242);
    pub const BORDER: Color32 = Color32::from_rgb(210, 210, 215);
    pub const SEPARATOR: Color32 = Color32::from_rgb(199, 199, 204);

    // MARK: - Text
    pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(29, 29, 31);
    pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(110, 110, 115);
    pub const TEXT_LINK: Color32 = ACCENT_BLUE;

    // MARK: - Semantic
    pub const GREEN_SUCCESS: Color32 = Color32::from_rgb(36, 138, 61);
    pub const RED_ERROR: Color32 = Color32::from_rgb(215, 58, 73);
    pub const RED_RECORDING: Color32 = Color32::from_rgb(255, 59, 48);
    pub const ORANGE_WARNING: Color32 = Color32::from_rgb(190, 105, 0);
    pub const YELLOW_CAUTION: Color32 = Color32::from_rgb(255, 149, 0);
}

/// Install the Apple-inspired theme into the egui context.
pub fn install(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // MARK: - Typography
    // Use system font with Apple-style hierarchy.
    // Proportional font is the system default (Segoe UI on Windows, SF on macOS).
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(22.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Name("Title".into()),
        egui::FontId::new(17.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(14.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(12.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Monospace,
        egui::FontId::new(13.0, egui::FontFamily::Monospace),
    );

    // MARK: - Spacing & layout
    style.animation_time = 0.18;
    style.spacing.item_spacing = egui::vec2(12.0, 12.0);
    style.spacing.window_margin = egui::Margin::same(24.0);
    style.spacing.button_padding = egui::vec2(18.0, 10.0);
    style.spacing.interact_size = egui::vec2(44.0, 34.0);
    style.spacing.indent = 16.0;
    style.spacing.menu_margin = egui::Margin::same(8.0);

    // MARK: - Visuals
    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = colors::BG_BASE;
    style.visuals.window_fill = colors::BG_CARD;
    style.visuals.extreme_bg_color = colors::BG_CARD_ALT;
    style.visuals.faint_bg_color = colors::FILL_SECONDARY;
    style.visuals.hyperlink_color = colors::TEXT_LINK;

    // Selection
    style.visuals.selection.bg_fill = colors::ACCENT_BLUE;
    style.visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);

    // Rounding
    style.visuals.window_rounding = egui::Rounding::same(14.0);
    style.visuals.menu_rounding = egui::Rounding::same(12.0);
    // popup_rounding not available in egui 0.27 — menu_rounding is used

    // Mark: - Widget states (noninteractive)
    style.visuals.widgets.noninteractive.bg_fill = colors::BG_CARD;
    style.visuals.widgets.noninteractive.weak_bg_fill = egui::Color32::TRANSPARENT;
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, colors::BORDER);
    style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, colors::TEXT_PRIMARY);
    style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(10.0);

    // Widget states (inactive)
    style.visuals.widgets.inactive.bg_fill = colors::BG_CARD;
    style.visuals.widgets.inactive.weak_bg_fill = colors::BG_CARD;
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, colors::BORDER);
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, colors::TEXT_PRIMARY);
    style.visuals.widgets.inactive.rounding = egui::Rounding::same(10.0);

    // Widget states (hovered) — subtle elevation
    style.visuals.widgets.hovered.bg_fill = colors::FILL_SECONDARY;
    style.visuals.widgets.hovered.weak_bg_fill = colors::FILL_SECONDARY;
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, colors::ACCENT_BLUE_HOVER);
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5, colors::TEXT_PRIMARY);
    style.visuals.widgets.hovered.rounding = egui::Rounding::same(10.0);
    style.visuals.widgets.hovered.expansion = 1.0;

    // Widget states (active / pressed)
    style.visuals.widgets.active.bg_fill = colors::ACCENT_BLUE_PRESSED;
    style.visuals.widgets.active.weak_bg_fill = colors::ACCENT_BLUE_PRESSED;
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, colors::ACCENT_BLUE_PRESSED);
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5, egui::Color32::WHITE);
    style.visuals.widgets.active.rounding = egui::Rounding::same(10.0);
    style.visuals.widgets.active.expansion = 0.0;

    // Widget states (open — dropdowns, etc.)
    style.visuals.widgets.open.bg_fill = colors::BG_CARD_ALT;
    style.visuals.widgets.open.weak_bg_fill = colors::BG_CARD_ALT;
    style.visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0, colors::ACCENT_BLUE);
    style.visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, colors::TEXT_PRIMARY);
    style.visuals.widgets.open.rounding = egui::Rounding::same(10.0);

    ctx.set_style(style);
}

/// A pill-shaped primary button with Apple-style blue fill.
/// Use for all primary actions (save, confirm, start recording).
pub fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(label)
            .fill(colors::ACCENT_BLUE)
            .stroke(egui::Stroke::new(1.0, colors::ACCENT_BLUE))
            .rounding(egui::Rounding::same(20.0)),
    )
}

/// A pill-shaped primary button with conditional enablement.
pub fn primary_button_enabled(ui: &mut egui::Ui, enabled: bool, label: &str) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(label)
            .fill(colors::ACCENT_BLUE)
            .stroke(egui::Stroke::new(1.0, colors::ACCENT_BLUE))
            .rounding(egui::Rounding::same(20.0)),
    )
}

/// A secondary (outline) button — subtle, for non-primary actions.
pub fn secondary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let theme_bg = ui.visuals().widgets.inactive.bg_fill;
    ui.add(
        egui::Button::new(label)
            .fill(theme_bg)
            .stroke(egui::Stroke::new(1.0, colors::BORDER))
            .rounding(egui::Rounding::same(20.0)),
    )
}

/// A destructive button (red text / outline) for irreversible actions.
pub fn destructive_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(label)
            .fill(egui::Color32::TRANSPARENT)
            .stroke(egui::Stroke::new(1.0, colors::RED_ERROR))
            .rounding(egui::Rounding::same(20.0)),
    )
}

/// Render a consistently-styled settings button (Apple-blue, pill-shaped).
#[allow(dead_code)]
pub fn settings_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(label)
            .fill(colors::ACCENT_BLUE)
            .stroke(egui::Stroke::new(1.0, colors::ACCENT_BLUE))
            .rounding(egui::Rounding::same(20.0)),
    )
}

/// Render a consistently-styled settings button with conditional enablement.
#[allow(dead_code)]
pub fn settings_button_enabled(ui: &mut egui::Ui, enabled: bool, label: &str) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(label)
            .fill(colors::ACCENT_BLUE)
            .stroke(egui::Stroke::new(1.0, colors::ACCENT_BLUE))
            .rounding(egui::Rounding::same(20.0)),
    )
}

/// Render a small badge-like label for status indication.
#[allow(dead_code)]
pub fn status_badge(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    let padding = egui::vec2(10.0, 4.0);
    let rounding = egui::Rounding::same(10.0);
    let monospace = egui::FontId::monospace(12.0);
    let row_height = ui.fonts(|f| f.row_height(&monospace));
    let text_width = ui.fonts(|f| {
        let galley = f.layout_no_wrap(text.to_owned(), monospace.clone(), color);
        galley.rect.width()
    });
    let total_width = padding.x * 2.0 + text_width;
    let total_height = padding.y * 2.0 + row_height;
    let (rect, _response) =
        ui.allocate_exact_size(egui::vec2(total_width, total_height), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, rounding, color.gamma_multiply(0.15));
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            monospace,
            color,
        );
    }
}

// ---------------------------------------------------------------------------
// Section card helper — renders a visual card container for grouped UI.

/// Begin a card section with an optional title.
/// Returns an egui::Ui::Id reference for adding child widgets.
/// Call `card_end(ui)` after adding children.
pub fn card_begin(ui: &mut egui::Ui, title: Option<&str>) {
    let available = ui.available_width();
    egui::Frame::none()
        .fill(colors::BG_CARD)
        .rounding(egui::Rounding::same(12.0))
        .stroke(egui::Stroke::new(1.0, colors::SEPARATOR))
        .inner_margin(egui::Margin::symmetric(16.0, 14.0))
        .show(ui, |ui| {
            ui.set_max_width(available);
            if let Some(title) = title {
                ui.label(
                    egui::RichText::new(title)
                        .size(15.0)
                        .color(colors::TEXT_PRIMARY)
                        .strong(),
                );
                ui.add_space(8.0);
            }
        });
}

/// End the card section (just adds closing space).
pub fn card_end(ui: &mut egui::Ui) {
    ui.add_space(2.0);
}

/// Section header with a subtle divider.
pub fn section_header(ui: &mut egui::Ui, text: &str) {
    ui.add_space(16.0);
    ui.label(
        egui::RichText::new(text)
            .size(17.0)
            .color(colors::TEXT_PRIMARY)
            .strong(),
    );
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(12.0);
}

/// Small caption text (secondary color).
pub fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(
        egui::RichText::new(text)
            .size(12.0)
            .color(colors::TEXT_SECONDARY),
    );
}

// ---------------------------------------------------------------------------
// Status indicator widgets

/// Draw a pulsing red dot (recording indicator).
pub fn recording_dot(ui: &mut egui::Ui, is_recording: bool) {
    if !is_recording {
        return;
    }
    let size = 10.0;
    let (rect, _response) =
        ui.allocate_exact_size(egui::vec2(size + 4.0, size + 4.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    // Outer glow
    painter.circle_filled(
        center,
        size * 0.8,
        colors::RED_RECORDING.gamma_multiply(0.3),
    );
    // Inner dot
    painter.circle_filled(center, size * 0.45, colors::RED_RECORDING);
}

/// Draw a success checkmark dot.
#[allow(dead_code)]
pub fn success_dot(ui: &mut egui::Ui) {
    let size = 10.0;
    let (rect, _response) =
        ui.allocate_exact_size(egui::vec2(size + 4.0, size + 4.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    painter.circle_filled(center, size * 0.4, colors::GREEN_SUCCESS);
}

/// Draw a cloud/processing dot.
pub fn processing_dot(ui: &mut egui::Ui) {
    let size = 10.0;
    let (rect, _response) =
        ui.allocate_exact_size(egui::vec2(size + 4.0, size + 4.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    painter.circle_filled(center, size * 0.4, colors::ACCENT_BLUE);
}

/// Draw a warning triangle indicator.
#[allow(dead_code)]
pub fn warning_indicator(ui: &mut egui::Ui, has_warning: bool) {
    if !has_warning {
        return;
    }
    let size = 10.0;
    let (rect, _response) =
        ui.allocate_exact_size(egui::vec2(size + 4.0, size + 4.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    painter.circle_filled(center, size * 0.4, colors::ORANGE_WARNING);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_button_helpers_have_correct_signatures() {
        let _: fn(&mut egui::Ui, &str) -> egui::Response = settings_button;
        let _: fn(&mut egui::Ui, bool, &str) -> egui::Response = settings_button_enabled;
    }

    #[test]
    fn primary_button_functions_compile() {
        let _: fn(&mut egui::Ui, &str) -> egui::Response = primary_button;
        let _: fn(&mut egui::Ui, bool, &str) -> egui::Response = primary_button_enabled;
        let _: fn(&mut egui::Ui, &str) -> egui::Response = secondary_button;
        let _: fn(&mut egui::Ui, &str) -> egui::Response = destructive_button;
    }

    #[test]
    fn colors_are_consistent() {
        assert_eq!(colors::TEXT_LINK, colors::ACCENT_BLUE);
        assert!(colors::BG_BASE != colors::BG_CARD);
    }
}
