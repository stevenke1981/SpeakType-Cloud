use crate::app::SpeakTypeCloudApp;
use crate::config::AppConfig;
use crate::secrets;
use eframe::egui;

pub struct AppleShell {
    app: SpeakTypeCloudApp,
    api_key_window_open: bool,
    show_api_keys: bool,
    openai_key_edit: String,
    xai_key_edit: String,
    key_message: Option<KeyMessage>,
    startup_warning: Option<String>,
}

struct KeyMessage {
    success: bool,
    text: String,
}

#[derive(Clone, Copy)]
enum ProviderKey {
    OpenAi,
    Xai,
}

#[derive(Clone, Copy)]
enum KeyAction {
    Save(ProviderKey),
    Clear(ProviderKey),
}

impl AppleShell {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::theme::install(&cc.egui_ctx);
        let startup_warning = AppConfig::load()
            .and_then(|config| secrets::hydrate_process_environment(&config))
            .err()
            .map(|error| error.to_string());

        Self {
            app: SpeakTypeCloudApp::new(cc),
            api_key_window_open: false,
            show_api_keys: false,
            openai_key_edit: String::new(),
            xai_key_edit: String::new(),
            key_message: None,
            startup_warning,
        }
    }

    fn show_api_key_launcher(&mut self, ctx: &egui::Context) {
        egui::Area::new(egui::Id::new("api-key-launcher"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-18.0, 18.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                if ui
                    .add(egui::Button::new(
                        egui::RichText::new("🔑  API 金鑰").strong(),
                    ))
                    .on_hover_text("設定 OpenAI 與 xAI API Key")
                    .clicked()
                {
                    self.api_key_window_open = true;
                    self.key_message = None;
                }
            });
    }

    fn show_api_key_window(&mut self, ctx: &egui::Context) {
        if !self.api_key_window_open {
            return;
        }

        let (openai_env, xai_env) = match configured_key_names() {
            Ok(names) => names,
            Err(error) => {
                self.key_message = Some(KeyMessage {
                    success: false,
                    text: error,
                });
                ("OPENAI_API_KEY".to_string(), "XAI_API_KEY".to_string())
            }
        };
        let openai_configured = secrets::is_api_key_configured(&openai_env);
        let xai_configured = secrets::is_api_key_configured(&xai_env);
        let mut action = None;
        let mut open = self.api_key_window_open;

        egui::Window::new("API 金鑰")
            .id(egui::Id::new("api-key-settings-window"))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("連接語音辨識服務")
                        .size(20.0)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(
                        "金鑰只會儲存在目前 Windows 使用者的環境變數，不會寫入 config.toml。",
                    )
                    .color(egui::Color32::from_rgb(110, 110, 115)),
                );
                ui.add_space(8.0);

                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("OpenAI").size(17.0).strong());
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                let (label, color) = configured_badge(openai_configured);
                                ui.colored_label(color, label);
                            },
                        );
                    });
                    ui.label(
                        egui::RichText::new(format!("環境變數：{openai_env}"))
                            .small()
                            .color(egui::Color32::from_rgb(110, 110, 115)),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.openai_key_edit)
                            .password(!self.show_api_keys)
                            .hint_text("貼上 OpenAI API Key")
                            .desired_width(f32::INFINITY),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("儲存 OpenAI Key").clicked() {
                            action = Some(KeyAction::Save(ProviderKey::OpenAi));
                        }
                        if ui
                            .add_enabled(openai_configured, egui::Button::new("清除"))
                            .clicked()
                        {
                            action = Some(KeyAction::Clear(ProviderKey::OpenAi));
                        }
                    });
                });

                ui.add_space(6.0);
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("xAI").size(17.0).strong());
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                let (label, color) = configured_badge(xai_configured);
                                ui.colored_label(color, label);
                            },
                        );
                    });
                    ui.label(
                        egui::RichText::new(format!("環境變數：{xai_env}"))
                            .small()
                            .color(egui::Color32::from_rgb(110, 110, 115)),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.xai_key_edit)
                            .password(!self.show_api_keys)
                            .hint_text("貼上 xAI API Key")
                            .desired_width(f32::INFINITY),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("儲存 xAI Key").clicked() {
                            action = Some(KeyAction::Save(ProviderKey::Xai));
                        }
                        if ui
                            .add_enabled(xai_configured, egui::Button::new("清除"))
                            .clicked()
                        {
                            action = Some(KeyAction::Clear(ProviderKey::Xai));
                        }
                    });
                });

                ui.add_space(6.0);
                ui.checkbox(&mut self.show_api_keys, "顯示輸入中的 API Key");

                if let Some(warning) = &self.startup_warning {
                    ui.colored_label(
                        egui::Color32::from_rgb(190, 105, 0),
                        format!("啟動提醒：{warning}"),
                    );
                }
                if let Some(message) = &self.key_message {
                    let color = if message.success {
                        egui::Color32::from_rgb(36, 138, 61)
                    } else {
                        egui::Color32::from_rgb(215, 58, 73)
                    };
                    ui.colored_label(color, &message.text);
                }
            });

        self.api_key_window_open = open;
        if let Some(action) = action {
            self.apply_key_action(action);
        }
    }

    fn apply_key_action(&mut self, action: KeyAction) {
        let config = match AppConfig::load().and_then(|config| {
            config.validate()?;
            Ok(config)
        }) {
            Ok(config) => config,
            Err(error) => {
                self.key_message = Some(KeyMessage {
                    success: false,
                    text: error.to_string(),
                });
                return;
            }
        };

        let (provider_name, env_name, key_value) = match action {
            KeyAction::Save(ProviderKey::OpenAi) => (
                "OpenAI",
                config.openai.api_key_env,
                Some(self.openai_key_edit.trim().to_string()),
            ),
            KeyAction::Save(ProviderKey::Xai) => (
                "xAI",
                config.xai.api_key_env,
                Some(self.xai_key_edit.trim().to_string()),
            ),
            KeyAction::Clear(ProviderKey::OpenAi) => {
                ("OpenAI", config.openai.api_key_env, None)
            }
            KeyAction::Clear(ProviderKey::Xai) => ("xAI", config.xai.api_key_env, None),
        };

        let result = match key_value {
            Some(api_key) => secrets::save_api_key(&env_name, &api_key),
            None => secrets::clear_api_key(&env_name),
        };

        match result {
            Ok(()) => {
                match action {
                    KeyAction::Save(ProviderKey::OpenAi) => self.openai_key_edit.clear(),
                    KeyAction::Save(ProviderKey::Xai) => self.xai_key_edit.clear(),
                    KeyAction::Clear(_) => {}
                }
                let verb = match action {
                    KeyAction::Save(_) => "已安全儲存",
                    KeyAction::Clear(_) => "已清除",
                };
                self.key_message = Some(KeyMessage {
                    success: true,
                    text: format!("{provider_name} API Key {verb}。"),
                });
            }
            Err(error) => {
                self.key_message = Some(KeyMessage {
                    success: false,
                    text: error.to_string(),
                });
            }
        }
    }
}

impl eframe::App for AppleShell {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        eframe::App::update(&mut self.app, ctx, frame);
        self.show_api_key_launcher(ctx);
        self.show_api_key_window(ctx);
    }
}

fn configured_key_names() -> Result<(String, String), String> {
    let config = AppConfig::load().map_err(|error| error.to_string())?;
    config.validate().map_err(|error| error.to_string())?;
    Ok((config.openai.api_key_env, config.xai.api_key_env))
}

fn configured_badge(configured: bool) -> (&'static str, egui::Color32) {
    if configured {
        ("● 已設定", egui::Color32::from_rgb(36, 138, 61))
    } else {
        ("○ 尚未設定", egui::Color32::from_rgb(110, 110, 115))
    }
}
