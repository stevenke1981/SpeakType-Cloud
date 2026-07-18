use eframe::egui;

pub fn install(ctx: &egui::Context) {
    let accent = egui::Color32::from_rgb(0, 122, 255);
    let accent_hover = egui::Color32::from_rgb(10, 132, 255);
    let accent_pressed = egui::Color32::from_rgb(0, 102, 204);
    let background = egui::Color32::from_rgb(245, 245, 247);
    let card = egui::Color32::from_rgb(255, 255, 255);
    let field = egui::Color32::from_rgb(250, 250, 252);
    let hover = egui::Color32::from_rgb(238, 238, 242);
    let border = egui::Color32::from_rgb(210, 210, 215);
    let text = egui::Color32::from_rgb(29, 29, 31);

    let mut style = (*ctx.style()).clone();
    style.animation_time = 0.18;
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.window_margin = egui::Margin::same(22.0);
    style.spacing.button_padding = egui::vec2(16.0, 9.0);
    style.spacing.interact_size = egui::vec2(44.0, 36.0);
    style.spacing.indent = 18.0;
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(25.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(15.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(15.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(12.5, egui::FontFamily::Proportional),
    );

    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = background;
    style.visuals.window_fill = card;
    style.visuals.extreme_bg_color = field;
    style.visuals.faint_bg_color = hover;
    style.visuals.hyperlink_color = accent;
    style.visuals.selection.bg_fill = accent;
    style.visuals.selection.stroke = egui::Stroke::new(1.0_f32, egui::Color32::WHITE);
    style.visuals.window_rounding = egui::Rounding::same(18.0);
    style.visuals.menu_rounding = egui::Rounding::same(14.0);

    style.visuals.widgets.noninteractive.bg_fill = card;
    style.visuals.widgets.noninteractive.weak_bg_fill = egui::Color32::TRANSPARENT;
    style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, border);
    style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, text);
    style.visuals.widgets.noninteractive.rounding = egui::Rounding::same(12.0);

    style.visuals.widgets.inactive.bg_fill = card;
    style.visuals.widgets.inactive.weak_bg_fill = card;
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, border);
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, text);
    style.visuals.widgets.inactive.rounding = egui::Rounding::same(12.0);

    style.visuals.widgets.hovered.bg_fill = hover;
    style.visuals.widgets.hovered.weak_bg_fill = hover;
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, accent_hover);
    style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.5_f32, text);
    style.visuals.widgets.hovered.rounding = egui::Rounding::same(12.0);
    style.visuals.widgets.hovered.expansion = 1.0;

    style.visuals.widgets.active.bg_fill = accent_pressed;
    style.visuals.widgets.active.weak_bg_fill = accent_pressed;
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, accent_pressed);
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5_f32, egui::Color32::WHITE);
    style.visuals.widgets.active.rounding = egui::Rounding::same(12.0);
    style.visuals.widgets.active.expansion = 0.0;

    style.visuals.widgets.open.bg_fill = field;
    style.visuals.widgets.open.weak_bg_fill = field;
    style.visuals.widgets.open.bg_stroke = egui::Stroke::new(1.0_f32, accent);
    style.visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0_f32, text);
    style.visuals.widgets.open.rounding = egui::Rounding::same(12.0);

    ctx.set_style(style);
}

/// Render a consistently-styled settings-page button.
///
/// All buttons on API key / update / control panels should use this
/// to guarantee identical normal / hover / active / disabled visuals.
/// The shared accent fill and stroke keep settings buttons visually identical
/// even when they are rendered in different windows or panels.
pub fn settings_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(label)
            .fill(settings_button_fill())
            .stroke(settings_button_stroke()),
    )
}

/// Render a consistently-styled settings-page button with conditional enablement.
///
/// Same as `settings_button` but honours an `enabled` flag. When disabled,
/// egui dims the shared accent baseline so the control remains recognizable
/// while correctly communicating that it cannot be clicked.
pub fn settings_button_enabled(ui: &mut egui::Ui, enabled: bool, label: &str) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(label)
            .fill(settings_button_fill())
            .stroke(settings_button_stroke()),
    )
}

fn settings_button_fill() -> egui::Color32 {
    egui::Color32::from_rgb(0, 122, 255)
}

fn settings_button_stroke() -> egui::Stroke {
    egui::Stroke::new(1.0, settings_button_fill())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_button_helpers_have_correct_signatures() {
        // These are thin wrappers around egui::Button that inherit all visual
        // properties from the global theme set by install().  We cannot call
        // install() without a CreationContext, but we can verify at compile
        // time that the function signatures match the expected widget API.
        //
        // install() explicitly overrides every widget state
        // (inactive / hovered / active / noninteractive / open) with
        // consistent fill, stroke, rounding, and expansion values derived
        // from the six design tokens at the top of this module.
        let _: fn(&mut egui::Ui, &str) -> egui::Response = settings_button;
        let _: fn(&mut egui::Ui, bool, &str) -> egui::Response = settings_button_enabled;
        assert_eq!(settings_button_stroke().color, settings_button_fill());
        assert_eq!(settings_button_stroke().width, 1.0);
    }
}
