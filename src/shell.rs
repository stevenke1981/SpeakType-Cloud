use crate::app::SpeakTypeCloudApp;
use crate::config::AppConfig;
use crate::secrets;
use crate::updater::{self, StagedUpdate, UpdateManifest};
use eframe::egui;
use std::sync::mpsc::{self, Receiver};

pub struct AppleShell {
    app: SpeakTypeCloudApp,
    tray: Option<SystemTray>,
    exit_requested: bool,
    api_key_window_open: bool,
    show_api_keys: bool,
    openai_key_edit: String,
    xai_key_edit: String,
    openrouter_key_edit: String,
    key_message: Option<KeyMessage>,
    startup_warning: Option<String>,
    update_window_open: bool,
    update_state: UpdateState,
    update_rx: Option<Receiver<UpdateWorkerResult>>,
}

struct KeyMessage {
    success: bool,
    text: String,
}

#[derive(Clone, Copy)]
enum ProviderKey {
    OpenAi,
    Xai,
    OpenRouter,
}

#[derive(Clone, Copy)]
enum KeyAction {
    Save(ProviderKey),
    Clear(ProviderKey),
}

enum UpdateState {
    Disabled(String),
    Idle,
    Checking,
    UpToDate,
    Available(UpdateManifest),
    Staging(UpdateManifest),
    Staged(StagedUpdate),
    Launched,
    Error(String),
}

enum UpdateWorkerResult {
    Checked(Result<Option<UpdateManifest>, String>),
    Staged(Result<StagedUpdate, String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum UpdateActionKind {
    Check,
    Stage,
    Launch,
}

impl AppleShell {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::theme::install(&cc.egui_ctx);
        let mut startup_warning = AppConfig::load()
            .and_then(|config| {
                config.validate()?;
                secrets::hydrate_process_environment(&config)
            })
            .err()
            .map(|error| error.to_string());
        let tray = match SystemTray::new() {
            Ok(tray) => Some(tray),
            Err(error) => {
                append_warning(&mut startup_warning, &format!("系統匣不可用：{error}"));
                None
            }
        };

        Self {
            app: SpeakTypeCloudApp::new(cc),
            tray,
            exit_requested: false,
            api_key_window_open: false,
            show_api_keys: false,
            openai_key_edit: String::new(),
            xai_key_edit: String::new(),
            openrouter_key_edit: String::new(),
            key_message: None,
            startup_warning,
            update_window_open: false,
            update_state: match updater::configured_signer_cert_sha256() {
                Ok(_) => UpdateState::Idle,
                Err(error) => UpdateState::Disabled(error),
            },
            update_rx: None,
        }
    }

    fn handle_window_lifecycle(&mut self, ctx: &egui::Context) {
        if let Some(action) = self.tray.as_ref().and_then(SystemTray::poll_action) {
            match action {
                TrayAction::Show => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                TrayAction::Exit => self.request_exit(ctx),
            }
        }

        if ctx.input(|input| input.viewport().close_requested()) {
            match close_decision(self.tray.is_some(), self.exit_requested) {
                CloseDecision::Hide => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                }
                CloseDecision::Exit => {}
            }
        }
    }

    fn request_exit(&mut self, ctx: &egui::Context) {
        self.exit_requested = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn show_window_controls(&mut self, ctx: &egui::Context) {
        let mut hide = false;
        let mut exit = false;
        egui::Area::new(egui::Id::new("window-lifecycle-controls"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-18.0, -18.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.tray.is_some(), egui::Button::new("隱藏到系統匣"))
                        .on_disabled_hover_text("系統匣初始化失敗，請使用退出程式")
                        .clicked()
                    {
                        hide = true;
                    }
                    if ui.button("退出程式").clicked() {
                        exit = true;
                    }
                });
            });
        if hide {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        if exit {
            self.request_exit(ctx);
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
                    .on_hover_text("設定 OpenAI、xAI 與 OpenRouter API Key")
                    .clicked()
                {
                    self.api_key_window_open = true;
                    self.key_message = None;
                }
            });
    }

    fn show_update_launcher(&mut self, ctx: &egui::Context) {
        egui::Area::new(egui::Id::new("update-launcher"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-18.0, 58.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                if ui.button("檢查更新").clicked() {
                    self.update_window_open = true;
                }
            });
    }

    fn poll_update_worker(&mut self) {
        let result = self.update_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        let Some(result) = result else { return };
        self.update_rx = None;
        self.update_state = match result {
            UpdateWorkerResult::Checked(Ok(Some(manifest))) => UpdateState::Available(manifest),
            UpdateWorkerResult::Checked(Ok(None)) => UpdateState::UpToDate,
            UpdateWorkerResult::Checked(Err(error)) | UpdateWorkerResult::Staged(Err(error)) => {
                UpdateState::Error(error)
            }
            UpdateWorkerResult::Staged(Ok(staged)) => UpdateState::Staged(staged),
        };
    }

    fn start_update_check(&mut self) {
        if !update_action_allowed(&self.update_state, UpdateActionKind::Check) {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.update_rx = Some(rx);
        self.update_state = UpdateState::Checking;
        std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())
                .and_then(|runtime| runtime.block_on(updater::check_for_update()));
            let _ = tx.send(UpdateWorkerResult::Checked(result));
        });
    }

    fn start_update_stage(&mut self, manifest: UpdateManifest) {
        if !update_action_allowed(&self.update_state, UpdateActionKind::Stage) {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.update_rx = Some(rx);
        self.update_state = UpdateState::Staging(manifest.clone());
        std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|error| error.to_string())
                .and_then(|runtime| runtime.block_on(updater::stage_update(&manifest)));
            let _ = tx.send(UpdateWorkerResult::Staged(result));
        });
    }

    fn show_update_window(&mut self, ctx: &egui::Context) {
        if !self.update_window_open {
            return;
        }
        let mut open = self.update_window_open;
        let mut action = None;
        egui::Window::new("SpeakType Cloud 更新")
            .id(egui::Id::new("update-window"))
            .collapsible(false)
            .resizable(false)
            .default_width(500.0)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(format!("目前版本：{}", env!("CARGO_PKG_VERSION")));
                ui.label("更新只會在您手動操作時下載；啟動安裝精靈前會再次確認與驗證。");
                ui.add_space(8.0);
                match &self.update_state {
                    UpdateState::Disabled(reason) => {
                        ui.colored_label(egui::Color32::from_rgb(190, 105, 0), reason);
                        ui.hyperlink_to(
                            "手動開啟 GitHub Releases",
                            "https://github.com/stevenke1981/SpeakType-Cloud/releases",
                        );
                    }
                    UpdateState::Idle => {
                        if ui.button("檢查 GitHub Releases").clicked() {
                            action = Some(UpdateAction::Check);
                        }
                    }
                    UpdateState::Checking => {
                        ui.spinner();
                        ui.label("正在檢查更新…");
                    }
                    UpdateState::UpToDate => {
                        ui.label("目前已是最新版本。");
                        if ui.button("再次檢查").clicked() {
                            action = Some(UpdateAction::Check);
                        }
                    }
                    UpdateState::Available(manifest) => {
                        ui.strong(format!("可用版本：{}", manifest.version));
                        ui.label("按下後才會下載至暫存資料夾並驗證 SHA-256 與 Authenticode 狀態。");
                        if ui.button("下載並驗證").clicked() {
                            action = Some(UpdateAction::Stage(manifest.clone()));
                        }
                    }
                    UpdateState::Staging(manifest) => {
                        ui.spinner();
                        ui.label(format!("正在下載並驗證 {}…", manifest.version));
                    }
                    UpdateState::Staged(staged) => {
                        ui.strong(format!("版本 {} 已驗證完成", staged.version));
                        ui.label("Authenticode：簽章有效，且簽署憑證符合內建信任根");
                        ui.label(format!("暫存路徑：{}", staged.installer_path.display()));
                        ui.colored_label(
                            egui::Color32::from_rgb(190, 105, 0),
                            "下一步會啟動可見的安裝精靈；不會靜默安裝。",
                        );
                        if ui.button("啟動安裝程式").clicked() {
                            action = Some(UpdateAction::Launch(staged.clone()));
                        }
                    }
                    UpdateState::Launched => {
                        ui.label("安裝精靈已啟動；請在安裝視窗中確認或取消。");
                    }
                    UpdateState::Error(error) => {
                        ui.colored_label(egui::Color32::from_rgb(215, 58, 73), error);
                        if ui.button("重新檢查").clicked() {
                            action = Some(UpdateAction::Check);
                        }
                    }
                }
            });
        self.update_window_open = open;
        match action {
            Some(UpdateAction::Check) => self.start_update_check(),
            Some(UpdateAction::Stage(manifest)) => self.start_update_stage(manifest),
            Some(UpdateAction::Launch(staged))
                if update_action_allowed(&self.update_state, UpdateActionKind::Launch) =>
            {
                match updater::launch_installer(&staged) {
                    Ok(()) => self.update_state = UpdateState::Launched,
                    Err(error) => self.update_state = UpdateState::Error(error),
                }
            }
            Some(UpdateAction::Launch(_)) => {}
            None => {}
        }
    }

    fn show_api_key_window(&mut self, ctx: &egui::Context) {
        if !self.api_key_window_open {
            return;
        }

        let (openai_env, xai_env, openrouter_env) = match configured_key_names() {
            Ok(names) => names,
            Err(error) => {
                self.key_message = Some(KeyMessage {
                    success: false,
                    text: error,
                });
                (
                    "OPENAI_API_KEY".to_string(),
                    "XAI_API_KEY".to_string(),
                    "OPENROUTER_API_KEY".to_string(),
                )
            }
        };
        let openai_configured = secrets::is_api_key_configured(&openai_env);
        let xai_configured = secrets::is_api_key_configured(&xai_env);
        let openrouter_configured = secrets::is_api_key_configured(&openrouter_env);
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
                ui.label(egui::RichText::new("連接語音辨識服務").size(20.0).strong());
                ui.label(
                    egui::RichText::new(
                        "金鑰會儲存在 Windows Credential Manager，不會寫入 config.toml；舊版使用者環境變數會在啟動時安全遷移。",
                    )
                    .color(egui::Color32::from_rgb(110, 110, 115)),
                );
                ui.add_space(8.0);

                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("OpenAI").size(17.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (label, color) = configured_badge(openai_configured);
                            ui.colored_label(color, label);
                        });
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
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (label, color) = configured_badge(xai_configured);
                            ui.colored_label(color, label);
                        });
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
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("OpenRouter").size(17.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (label, color) = configured_badge(openrouter_configured);
                            ui.colored_label(color, label);
                        });
                    });
                    ui.label(
                        egui::RichText::new(format!("環境變數：{openrouter_env}"))
                            .small()
                            .color(egui::Color32::from_rgb(110, 110, 115)),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.openrouter_key_edit)
                            .password(!self.show_api_keys)
                            .hint_text("貼上 OpenRouter API Key")
                            .desired_width(f32::INFINITY),
                    );
                    ui.horizontal(|ui| {
                        if ui.button("儲存 OpenRouter Key").clicked() {
                            action = Some(KeyAction::Save(ProviderKey::OpenRouter));
                        }
                        if ui
                            .add_enabled(openrouter_configured, egui::Button::new("清除"))
                            .clicked()
                        {
                            action = Some(KeyAction::Clear(ProviderKey::OpenRouter));
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
            KeyAction::Save(ProviderKey::OpenRouter) => (
                "OpenRouter",
                config.openrouter.api_key_env,
                Some(self.openrouter_key_edit.trim().to_string()),
            ),
            KeyAction::Clear(ProviderKey::OpenAi) => ("OpenAI", config.openai.api_key_env, None),
            KeyAction::Clear(ProviderKey::Xai) => ("xAI", config.xai.api_key_env, None),
            KeyAction::Clear(ProviderKey::OpenRouter) => {
                ("OpenRouter", config.openrouter.api_key_env, None)
            }
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
                    KeyAction::Save(ProviderKey::OpenRouter) => self.openrouter_key_edit.clear(),
                    KeyAction::Clear(_) => {}
                }
                let verb = match action {
                    KeyAction::Save(_) => "已儲存",
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
        self.handle_window_lifecycle(ctx);
        self.poll_update_worker();
        eframe::App::update(&mut self.app, ctx, frame);
        self.show_api_key_launcher(ctx);
        self.show_update_launcher(ctx);
        self.show_api_key_window(ctx);
        self.show_update_window(ctx);
        self.show_window_controls(ctx);
    }
}

enum UpdateAction {
    Check,
    Stage(UpdateManifest),
    Launch(StagedUpdate),
}

fn update_action_allowed(state: &UpdateState, action: UpdateActionKind) -> bool {
    matches!(
        (state, action),
        (
            UpdateState::Idle | UpdateState::UpToDate | UpdateState::Error(_),
            UpdateActionKind::Check
        ) | (UpdateState::Available(_), UpdateActionKind::Stage)
            | (UpdateState::Staged(_), UpdateActionKind::Launch)
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseDecision {
    Hide,
    Exit,
}

fn close_decision(tray_available: bool, exit_requested: bool) -> CloseDecision {
    if tray_available && !exit_requested {
        CloseDecision::Hide
    } else {
        CloseDecision::Exit
    }
}

fn append_warning(current: &mut Option<String>, warning: &str) {
    if let Some(current) = current {
        current.push('\n');
        current.push_str(warning);
    } else {
        *current = Some(warning.to_string());
    }
}

#[derive(Clone, Copy)]
enum TrayAction {
    Show,
    Exit,
}

#[cfg(target_os = "windows")]
struct SystemTray {
    _icon: tray_icon::TrayIcon,
    tray_id: tray_icon::TrayIconId,
    show_id: tray_icon::menu::MenuId,
    exit_id: tray_icon::menu::MenuId,
}

#[cfg(target_os = "windows")]
impl SystemTray {
    fn new() -> Result<Self, String> {
        use tray_icon::menu::{Menu, MenuItem};
        use tray_icon::TrayIconBuilder;

        let show = MenuItem::new("顯示 SpeakType Cloud", true, None);
        let exit = MenuItem::new("退出", true, None);
        let menu = Menu::with_items(&[&show, &exit]).map_err(|error| error.to_string())?;
        let icon = tray_icon_image()?;
        let tray = TrayIconBuilder::new()
            .with_tooltip("SpeakType Cloud")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            tray_id: tray.id().clone(),
            show_id: show.id().clone(),
            exit_id: exit.id().clone(),
            _icon: tray,
        })
    }

    fn poll_action(&self) -> Option<TrayAction> {
        use tray_icon::menu::MenuEvent;
        use tray_icon::{MouseButton, TrayIconEvent};

        let mut action = None;
        for event in MenuEvent::receiver().try_iter() {
            if event.id == self.exit_id {
                return Some(TrayAction::Exit);
            }
            if event.id == self.show_id {
                action = Some(TrayAction::Show);
            }
        }
        for event in TrayIconEvent::receiver().try_iter() {
            if matches!(
                event,
                TrayIconEvent::DoubleClick {
                    ref id,
                    button: MouseButton::Left,
                    ..
                } if id == &self.tray_id
            ) {
                action = Some(TrayAction::Show);
            }
        }
        action
    }
}

#[cfg(target_os = "windows")]
fn tray_icon_image() -> Result<tray_icon::Icon, String> {
    const SIZE: u32 = 32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as i32 - 16;
            let dy = y as i32 - 16;
            if dx * dx + dy * dy <= 14 * 14 {
                let waveform = (x == 12 && (9..=23).contains(&y))
                    || (x == 16 && (6..=26).contains(&y))
                    || (x == 20 && (10..=22).contains(&y));
                rgba.extend_from_slice(if waveform {
                    &[255, 255, 255, 255]
                } else {
                    &[42, 110, 242, 255]
                });
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    tray_icon::Icon::from_rgba(rgba, SIZE, SIZE).map_err(|error| error.to_string())
}

#[cfg(not(target_os = "windows"))]
struct SystemTray;

#[cfg(not(target_os = "windows"))]
impl SystemTray {
    fn new() -> Result<Self, String> {
        Err("系統匣目前僅支援 Windows".to_string())
    }

    fn poll_action(&self) -> Option<TrayAction> {
        None
    }
}

fn configured_key_names() -> Result<(String, String, String), String> {
    let config = AppConfig::load().map_err(|error| error.to_string())?;
    config.validate().map_err(|error| error.to_string())?;
    Ok((
        config.openai.api_key_env,
        config.xai.api_key_env,
        config.openrouter.api_key_env,
    ))
}

fn configured_badge(configured: bool) -> (&'static str, egui::Color32) {
    if configured {
        ("● 已設定", egui::Color32::from_rgb(36, 138, 61))
    } else {
        ("○ 尚未設定", egui::Color32::from_rgb(110, 110, 115))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_close_hides_when_tray_is_available() {
        assert_eq!(close_decision(true, false), CloseDecision::Hide);
    }

    #[test]
    fn explicit_exit_or_missing_tray_allows_close() {
        assert_eq!(close_decision(true, true), CloseDecision::Exit);
        assert_eq!(close_decision(false, false), CloseDecision::Exit);
    }

    #[test]
    fn updater_requires_check_stage_and_launch_confirmations_in_order() {
        assert!(update_action_allowed(
            &UpdateState::Idle,
            UpdateActionKind::Check
        ));
        assert!(!update_action_allowed(
            &UpdateState::Idle,
            UpdateActionKind::Stage
        ));
        let available = UpdateState::Available(UpdateManifest {
            schema_version: 1,
            version: "1.2.3".to_string(),
            installer_url:
                "https://github.com/stevenke1981/SpeakType-Cloud/releases/download/v1.2.3/a.exe"
                    .to_string(),
            sha256: "a".repeat(64),
        });
        assert!(update_action_allowed(&available, UpdateActionKind::Stage));
        assert!(!update_action_allowed(&available, UpdateActionKind::Launch));
        let staged = UpdateState::Staged(StagedUpdate {
            version: "1.2.3".to_string(),
            installer_path: std::env::temp_dir().join("SpeakTypeCloud-test.exe"),
            signer_cert_sha256: "a".repeat(64),
            expected_sha256: "a".repeat(64),
        });
        assert!(update_action_allowed(&staged, UpdateActionKind::Launch));
        assert!(!update_action_allowed(&staged, UpdateActionKind::Check));
        let disabled = UpdateState::Disabled("missing trust root".to_string());
        assert!(!update_action_allowed(&disabled, UpdateActionKind::Check));
        assert!(!update_action_allowed(&disabled, UpdateActionKind::Stage));
        assert!(!update_action_allowed(&disabled, UpdateActionKind::Launch));
    }
}
