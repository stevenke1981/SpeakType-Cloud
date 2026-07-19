use crate::app::SpeakTypeCloudApp;
use crate::config::AppConfig;
use crate::providers;
use crate::secrets;
use crate::updater::{self, StagedUpdate, UpdateManifest};
use eframe::egui;
use std::sync::mpsc::{self, Receiver};
use std::sync::Mutex;
use std::time::Duration;

#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::RECT;
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowLongPtrW, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId,
    IsWindowVisible, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, WS_EX_APPWINDOW,
    WS_EX_TOOLWINDOW,
};

/// Saved normal-window position, captured at hide time and restored on show.
/// (x, y, width, height) — only meaningful when off-screen is active.
#[cfg(target_os = "windows")]
static SAVED_RECT: Mutex<Option<(i32, i32, i32, i32)>> = Mutex::new(None);

/// Cached provider selection for clearing stale model fetch errors.
static PROVIDER_CACHE: Mutex<Option<crate::config::ProviderKind>> = Mutex::new(None);

/// Resolve the real application window on every lifecycle operation.
///
/// The eframe/winit process also owns transient renderer/helper windows. A
/// cached "first window" handle can therefore point at the wrong HWND. Walk
/// top-level windows, then require both this process's PID and the product
/// title. This also works after the main window is moved off-screen and has
/// its taskbar style changed.
#[cfg(target_os = "windows")]
unsafe fn find_main_hwnd() -> Option<*mut std::ffi::c_void> {
    unsafe extern "system" fn enum_callback(hwnd: *mut std::ffi::c_void, lparam: isize) -> i32 {
        let mut owner_pid = 0;
        unsafe {
            GetWindowThreadProcessId(hwnd, &mut owner_pid);
        }
        if owner_pid != unsafe { GetCurrentProcessId() } || unsafe { IsWindowVisible(hwnd) } == 0 {
            return 1;
        }

        let mut title = [0u16; 64];
        let length = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) };
        let expected: Vec<u16> = "SpeakType Cloud".encode_utf16().collect();
        if length > 0 && title[..length as usize] == expected[..] {
            unsafe {
                *(lparam as *mut *mut std::ffi::c_void) = hwnd;
            }
            0
        } else {
            1
        }
    }

    let mut candidate: *mut std::ffi::c_void = std::ptr::null_mut();
    unsafe {
        EnumWindows(
            Some(enum_callback),
            &mut candidate as *mut *mut std::ffi::c_void as isize,
        );
    }
    (!candidate.is_null()).then_some(candidate)
}

/// Save the current window rectangle, then move the window off-screen so it
/// is invisible despite being non-minimized and visible to the event loop.
/// Repeated calls while already hidden keep the original visible rectangle.
#[cfg(target_os = "windows")]
unsafe fn hide_window_offscreen() {
    if let Some(hwnd) = unsafe { find_main_hwnd() } {
        // 1. Save the current visible rect only once per hide/show cycle.
        let mut guard = SAVED_RECT.lock().expect("SAVED_RECT lock poisoned");
        if guard.is_none() {
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if unsafe { GetWindowRect(hwnd, &mut rect) } != 0 {
                *guard = Some((
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                ));
            }
        }
        drop(guard);
        // 2. Move off-screen (extreme negative coords) — window stays visible
        //    and non-minimised so eframe's event loop keeps running.
        unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                -32000,
                -32000,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

/// Restore the window to its saved position and size (captured by
/// `hide_window_offscreen`).  If no saved rect exists the call is a no-op.
/// The window is brought back to the visible desktop area.
#[cfg(target_os = "windows")]
unsafe fn show_window_restore() {
    let Some(hwnd) = (unsafe { find_main_hwnd() }) else {
        return;
    };
    let rect = { SAVED_RECT.lock().expect("SAVED_RECT lock poisoned").take() };
    if let Some((x, y, w, h)) = rect {
        let restored = unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                x,
                y,
                w,
                h,
                SWP_NOZORDER | SWP_SHOWWINDOW,
            )
        };
        if restored == 0 {
            // Keep the rectangle so a later tray event can retry rather than
            // permanently losing the user's last visible position.
            *SAVED_RECT.lock().expect("SAVED_RECT lock poisoned") = Some((x, y, w, h));
        }
    }
}

pub struct AppleShell {
    app: SpeakTypeCloudApp,
    tray: Option<SystemTray>,
    exit_requested: bool,
    window_hidden: bool,
    settings_window_open: bool,
    show_api_keys: bool,
    openai_key_edit: String,
    xai_key_edit: String,
    openrouter_key_edit: String,
    key_message: Option<KeyMessage>,
    startup_warning: Option<String>,
    update_window_open: bool,
    update_state: UpdateState,
    update_rx: Option<Receiver<UpdateWorkerResult>>,
    // Model list state
    openai_models: Vec<String>,
    openrouter_models: Vec<String>,
    models_loading: bool,
    models_error: Option<String>,
    models_rx: Option<Receiver<ModelsFetchResult>>,
}

#[derive(Clone)]
enum ModelsFetchResult {
    OpenAi(Vec<String>),
    OpenRouter(Vec<String>),
    Error(String),
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
        let tray = match SystemTray::new(&cc.egui_ctx) {
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
            window_hidden: false,
            settings_window_open: false,
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
            openai_models: Vec::new(),
            openrouter_models: Vec::new(),
            models_loading: false,
            models_error: None,
            models_rx: None,
        }
    }

    fn handle_window_lifecycle(&mut self, ctx: &egui::Context) {
        // Poll tray menu / click actions. Handlers wake the egui loop via
        // Context::request_repaint so this still runs while the window is hidden.
        if let Some(action) = self.tray.as_ref().and_then(SystemTray::poll_action) {
            match action {
                TrayAction::Show => {
                    self.show_from_tray(ctx);
                }
                TrayAction::Exit => {
                    // Reuse request_exit so all exit logic (exit_requested,
                    // Close command, backup repaint) stays in one place.
                    self.request_exit(ctx);
                }
            }
        }

        // Handle close-requested (window X button, or Close command from code).
        // When exit_requested is true we allow the close to proceed; otherwise
        // we intercept and hide to tray when available.
        if ctx.input(|input| input.viewport().close_requested()) {
            match close_decision(self.tray.is_some(), self.exit_requested) {
                CloseDecision::Hide => {
                    self.window_hidden = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    // Move the window off-screen and remove its taskbar button.
                    // This keeps the window non-minimised and visible-to-winit, so
                    // eframe 0.27's event loop keeps running and tray channel
                    // polling via update() continues to work.
                    #[cfg(target_os = "windows")]
                    unsafe {
                        hide_window_offscreen();
                        window_hide_ext_style();
                    }
                    ctx.request_repaint_after(Duration::from_millis(250));
                }
                CloseDecision::Exit => {
                    // Allow the native close to proceed; eframe will drop the
                    // App (AppleShell → SpeakTypeCloudApp), running all Rust
                    // Drop implementations for config handles, temp files etc.
                }
            }
        }

        // Keep a low-rate backup repaint so tray actions remain recoverable
        // even if a tray handler failed to wake the loop.
        //
        // Two cases must be covered:
        //   (a) window_hidden  — the egui event loop does not automatically
        //       repaint hidden windows.  Without this backup the tray would
        //       never be polled, making Show / Exit unrecoverable.
        //   (b) exit_requested — request_exit has sent Close but it has not
        //       yet been delivered.  The old guard !exit_requested stopped
        //       repainting here, which could stall the exit.

        // SAFETY for (b): once close_requested fires and
        // CloseDecision::Exit is taken, the app is dropped on that same
        // frame, so any outstanding repaint_after is harmless.
        if should_backup_repaint(self.window_hidden, self.exit_requested) {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn show_from_tray(&mut self, ctx: &egui::Context) {
        self.window_hidden = false;
        // Restore the app window position (undo off-screen) and extended style
        // (undo WS_EX_TOOLWINDOW) so the taskbar button reappears.
        #[cfg(target_os = "windows")]
        unsafe {
            show_window_restore();
            window_show_ext_style();
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }

    fn request_exit(&mut self, ctx: &egui::Context) {
        self.exit_requested = true;
        self.window_hidden = false;
        // ViewportCommand::Close works regardless of current visibility.
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        ctx.request_repaint();
    }

    fn show_window_controls(&mut self, ctx: &egui::Context) {
        let mut hide = false;
        let mut exit = false;
        egui::Area::new(egui::Id::new("window-lifecycle-controls"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-18.0, -18.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if crate::theme::secondary_button(ui, "隱藏到系統匣")
                        .on_disabled_hover_text("系統匣初始化失敗，請使用退出程式")
                        .clicked()
                    {
                        hide = true;
                    }
                    if crate::theme::destructive_button(ui, "退出程式").clicked() {
                        exit = true;
                    }
                });
            });
        if hide {
            self.window_hidden = true;
            #[cfg(target_os = "windows")]
            unsafe {
                hide_window_offscreen();
                window_hide_ext_style();
            }
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if exit {
            self.request_exit(ctx);
        }
    }

    fn show_settings_launcher(&mut self, ctx: &egui::Context) {
        egui::Area::new(egui::Id::new("settings-launcher"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-18.0, 18.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                if crate::theme::secondary_button(ui, "設定")
                    .on_hover_text("供應商、模型、API Key 與辨識設定")
                    .clicked()
                {
                    self.settings_window_open = true;
                    self.key_message = None;
                }
            });
    }

    fn show_update_launcher(&mut self, ctx: &egui::Context) {
        egui::Area::new(egui::Id::new("update-launcher"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-18.0, 58.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                if crate::theme::secondary_button(ui, "檢查更新").clicked() {
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

    fn poll_model_fetch(&mut self) {
        let result = self.models_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        let Some(result) = result else { return };
        self.models_rx = None;
        self.models_loading = false;
        match result {
            ModelsFetchResult::OpenAi(models) => {
                self.openai_models = models;
                self.models_error = None;
            }
            ModelsFetchResult::OpenRouter(models) => {
                self.openrouter_models = models;
                self.models_error = None;
            }
            ModelsFetchResult::Error(error) => {
                self.models_error = Some(error);
            }
        }
    }

    fn start_fetch_models(&mut self, provider: ProviderKey) {
        if self.models_loading {
            return;
        }
        let (env_name, base_url) = match provider {
            ProviderKey::OpenAi => (
                self.app.config.openai.api_key_env.clone(),
                self.app.config.openai.base_url.clone(),
            ),
            ProviderKey::OpenRouter => (
                self.app.config.openrouter.api_key_env.clone(),
                self.app.config.openrouter.base_url.clone(),
            ),
            ProviderKey::Xai => return, // xAI doesn't expose model selection
        };
        let api_key = match std::env::var(&env_name) {
            Ok(k) => k,
            Err(_) => {
                self.models_error = Some(format!("找不到 {env_name}，請先設定該供應商的 API Key"));
                return;
            }
        };
        let (tx, rx) = mpsc::channel();
        self.models_rx = Some(rx);
        self.models_loading = true;
        self.models_error = None;
        std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| e.to_string())
                .and_then(|rt| {
                    rt.block_on(providers::fetch_available_models(&base_url, &api_key))
                        .map_err(|e| e.to_string())
                });
            let msg = match (&result, provider) {
                (Ok(models), ProviderKey::OpenAi) => ModelsFetchResult::OpenAi(models.clone()),
                (Ok(models), ProviderKey::OpenRouter) => {
                    ModelsFetchResult::OpenRouter(models.clone())
                }
                (Ok(_), ProviderKey::Xai) => return,
                (Err(error), _) => ModelsFetchResult::Error(error.clone()),
            };
            let _ = tx.send(msg);
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
                        ui.colored_label(crate::theme::colors::ORANGE_WARNING, reason);
                        ui.hyperlink_to(
                            "手動開啟 GitHub Releases",
                            "https://github.com/stevenke1981/SpeakType-Cloud/releases",
                        );
                    }
                    UpdateState::Idle => {
                        if crate::theme::primary_button(ui, "檢查 GitHub Releases").clicked() {
                            action = Some(UpdateAction::Check);
                        }
                    }
                    UpdateState::Checking => {
                        ui.spinner();
                        ui.label("正在檢查更新…");
                    }
                    UpdateState::UpToDate => {
                        ui.label("目前已是最新版本。");
                        if crate::theme::primary_button(ui, "再次檢查").clicked() {
                            action = Some(UpdateAction::Check);
                        }
                    }
                    UpdateState::Available(manifest) => {
                        ui.strong(format!("可用版本：{}", manifest.version));
                        ui.label("按下後才會下載至暫存資料夾並驗證 SHA-256 與 Authenticode 狀態。");
                        if crate::theme::primary_button(ui, "下載並驗證").clicked() {
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
                            crate::theme::colors::ORANGE_WARNING,
                            "下一步會啟動可見的安裝精靈；不會靜默安裝。",
                        );
                        if crate::theme::primary_button(ui, "啟動安裝程式").clicked() {
                            action = Some(UpdateAction::Launch(staged.clone()));
                        }
                    }
                    UpdateState::Launched => {
                        ui.label("安裝精靈已啟動；請在安裝視窗中確認或取消。");
                    }
                    UpdateState::Error(error) => {
                        ui.colored_label(crate::theme::colors::RED_ERROR, error);
                        if crate::theme::primary_button(ui, "重新檢查").clicked() {
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

    fn show_settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_window_open {
            return;
        }

        // Clear stale model errors when settings opens
        let provider_changed = {
            let prev = PROVIDER_CACHE
                .lock()
                .expect("PROVIDER_CACHE lock poisoned")
                .replace(self.app.config.provider);
            prev.is_some() && prev != Some(self.app.config.provider)
        };
        if provider_changed {
            self.models_error = None;
        }

        let openai_env = self.app.config.openai.api_key_env.clone();
        let xai_env = self.app.config.xai.api_key_env.clone();
        let openrouter_env = self.app.config.openrouter.api_key_env.clone();
        let openai_configured = secrets::is_api_key_configured(&openai_env);
        let xai_configured = secrets::is_api_key_configured(&xai_env);
        let openrouter_configured = secrets::is_api_key_configured(&openrouter_env);
        let mut key_action = None;
        let mut config_save = false;
        let mut open = self.settings_window_open;

        egui::Window::new("設定")
            .id(egui::Id::new("settings-window"))
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(false)
            .default_width(720.0)
            .open(&mut open)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(720.0)
                    .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("API 與辨識設定")
                        .size(20.0)
                        .color(crate::theme::colors::TEXT_PRIMARY)
                        .strong(),
                );
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(8.0);

                egui::ComboBox::from_label("辨識模式")
                    .selected_text(self.app.config.transcription_mode.label())
                    .show_ui(ui, |ui| {
                        for mode in [
                            crate::config::TranscriptionMode::BatchPtt,
                            crate::config::TranscriptionMode::RealtimePtt,
                            crate::config::TranscriptionMode::ContinuousDictation,
                        ] {
                            ui.selectable_value(
                                &mut self.app.config.transcription_mode,
                                mode,
                                mode.label(),
                            );
                        }
                    });
                egui::ComboBox::from_label("供應商")
                    .selected_text(self.app.config.provider.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.app.config.provider,
                            crate::config::ProviderKind::OpenAi,
                            "OpenAI",
                        );
                        ui.selectable_value(
                            &mut self.app.config.provider,
                            crate::config::ProviderKind::Xai,
                            "xAI",
                        );
                        ui.selectable_value(
                            &mut self.app.config.provider,
                            crate::config::ProviderKind::OpenRouter,
                            "OpenRouter",
                        );
                    });
                ui.horizontal(|ui| {
                    ui.label("語言代碼");
                    ui.text_edit_singleline(&mut self.app.config.language);
                });
                ui.label("提示詞／詞彙背景");
                ui.text_edit_multiline(&mut self.app.config.prompt);

                // Provider-specific settings
                match self.app.config.provider {
                    crate::config::ProviderKind::OpenAi => {
                        ui.horizontal(|ui| {
                            ui.label("模型");
                            if self.openai_models.is_empty() {
                                ui.text_edit_singleline(&mut self.app.config.openai.model);
                            } else {
                                egui::ComboBox::from_id_source("openai-model")
                                    .selected_text(&self.app.config.openai.model)
                                    .show_ui(ui, |cb_ui| {
                                        for model in &self.openai_models {
                                            cb_ui.selectable_value(
                                                &mut self.app.config.openai.model,
                                                model.clone(),
                                                model.as_str(),
                                            );
                                        }
                                    });
                            }
                            if self.models_loading {
                                ui.add(egui::Spinner::new());
                            } else if crate::theme::secondary_button(ui, "↻ 重新整理").clicked() {
                                self.start_fetch_models(ProviderKey::OpenAi);
                            }
                        });
                        if self.app.config.transcription_mode.is_realtime() {
                            ui.horizontal(|ui| {
                                ui.label("Realtime 模型");
                                ui.text_edit_singleline(
                                    &mut self.app.config.realtime.openai_model,
                                );
                            });
                            egui::ComboBox::from_label("Transcription delay")
                                .selected_text(
                                    self.app.config.realtime.openai_transcription_delay.label(),
                                )
                                .show_ui(ui, |ui| {
                                    for delay in [
                                        crate::config::OpenAiTranscriptionDelay::Minimal,
                                        crate::config::OpenAiTranscriptionDelay::Low,
                                        crate::config::OpenAiTranscriptionDelay::Medium,
                                        crate::config::OpenAiTranscriptionDelay::High,
                                        crate::config::OpenAiTranscriptionDelay::XHigh,
                                    ] {
                                        ui.selectable_value(
                                            &mut self.app.config.realtime.openai_transcription_delay,
                                            delay,
                                            delay.label(),
                                        );
                                    }
                                });
                            ui.label("OpenAI gpt-realtime-whisper 使用本地 VAD，不啟用 server VAD。");
                        }
                    }
                    crate::config::ProviderKind::OpenRouter => {
                        ui.horizontal(|ui| {
                            ui.label("模型");
                            if self.openrouter_models.is_empty() {
                                ui.text_edit_singleline(&mut self.app.config.openrouter.model);
                            } else {
                                egui::ComboBox::from_id_source("openrouter-model")
                                    .selected_text(&self.app.config.openrouter.model)
                                    .show_ui(ui, |cb_ui| {
                                        for model in &self.openrouter_models {
                                            cb_ui.selectable_value(
                                                &mut self.app.config.openrouter.model,
                                                model.clone(),
                                                model.as_str(),
                                            );
                                        }
                                    });
                            }
                            if self.models_loading {
                                ui.add(egui::Spinner::new());
                            } else if crate::theme::secondary_button(ui, "↻ 重新整理").clicked() {
                                self.start_fetch_models(ProviderKey::OpenRouter);
                            }
                        });
                        if self.app.config.transcription_mode.is_realtime() {
                            ui.colored_label(
                                crate::theme::colors::ORANGE_WARNING,
                                "OpenRouter 僅支援 Batch / PTT 模式。請切換至 Batch / PTT。",
                            );
                        } else {
                            ui.label("OpenRouter 僅支援 Batch / PTT 模式；選用 Realtime 模式時，設定驗證會拒絕。");
                        }
                    }
                    crate::config::ProviderKind::Xai => {
                        ui.checkbox(
                            &mut self.app.config.xai.format_text,
                            "支援的語言啟用文字格式化",
                        );
                        ui.label("xAI Keyterms（每行一個）");
                        ui.text_edit_multiline(&mut self.app.keyterms_edit);
                        if self.app.config.transcription_mode.is_realtime() {
                            ui.checkbox(
                                &mut self.app.config.realtime.xai_smart_turn_enabled,
                                "使用 xAI server Smart Turn（選配）",
                            );
                            if self.app.config.realtime.xai_smart_turn_enabled {
                                ui.add(
                                    egui::Slider::new(
                                        &mut self.app.config.realtime.xai_smart_turn_threshold,
                                        0.0..=1.0,
                                    )
                                    .text("Smart Turn threshold"),
                                );
                                ui.add(
                                    egui::Slider::new(
                                        &mut self.app.config.realtime.xai_smart_turn_timeout_ms,
                                        1..=5_000,
                                    )
                                    .text("Smart Turn timeout (ms)"),
                                );
                            }
                        }
                    }
                }
                if let Some(err) = &self.models_error {
                    ui.colored_label(crate::theme::colors::RED_ERROR, err);
                }
                ui.checkbox(
                    &mut self.app.config.text_processing.normalize_chinese_punctuation,
                    "正規化中文標點",
                );
                if self.app.config.transcription_mode
                    == crate::config::TranscriptionMode::ContinuousDictation
                {
                    ui.add(
                        egui::Slider::new(
                            &mut self.app.config.realtime.vad_rms_threshold,
                            0.001..=0.2,
                        )
                        .text("本地 VAD RMS threshold"),
                    );
                    ui.add(
                        egui::Slider::new(
                            &mut self.app.config.realtime.vad_silence_ms,
                            100..=3_000,
                        )
                        .text("句尾靜音 (ms)"),
                    );
                }
                egui::ComboBox::from_label("中文輸出")
                    .selected_text(self.app.config.text_processing.chinese_variant.label())
                    .show_ui(ui, |ui| {
                        for variant in [
                            crate::config::ChineseVariant::Preserve,
                            crate::config::ChineseVariant::Traditional,
                            crate::config::ChineseVariant::Simplified,
                        ] {
                            ui.selectable_value(
                                &mut self.app.config.text_processing.chinese_variant,
                                variant,
                                variant.label(),
                            );
                        }
                    });
                ui.checkbox(
                    &mut self.app.config.text_processing.voice_commands_enabled,
                    "啟用語音命令（僅完整片語匹配）",
                );

                crate::theme::section_header(ui, "API 金鑰");
                ui.label(
                    egui::RichText::new(
                        "金鑰會儲存在 Windows Credential Manager，不會寫入 config.toml；舊版使用者環境變數會在啟動時安全遷移。",
                    )
                    .size(12.0)
                    .color(crate::theme::colors::TEXT_SECONDARY),
                );
                ui.add_space(10.0);

                // OpenAI card
                crate::theme::card_begin(ui, None);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("OpenAI").size(17.0).color(crate::theme::colors::TEXT_PRIMARY).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (label, color) = configured_badge(openai_configured);
                        ui.colored_label(color, label);
                    });
                });
                crate::theme::caption(ui, &format!("環境變數：{openai_env}"));
                ui.add_space(4.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.openai_key_edit)
                        .password(!self.show_api_keys)
                        .hint_text("貼上 OpenAI API Key")
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal(|ui| {
                    if crate::theme::primary_button(ui, "儲存 OpenAI Key").clicked() {
                        key_action = Some(KeyAction::Save(ProviderKey::OpenAi));
                    }
                    if crate::theme::secondary_button(ui, "清除").clicked()
                        && openai_configured
                    {
                        key_action = Some(KeyAction::Clear(ProviderKey::OpenAi));
                    }
                });
                crate::theme::card_end(ui);

                ui.add_space(8.0);
                // xAI card
                crate::theme::card_begin(ui, None);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("xAI").size(17.0).color(crate::theme::colors::TEXT_PRIMARY).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (label, color) = configured_badge(xai_configured);
                        ui.colored_label(color, label);
                    });
                });
                crate::theme::caption(ui, &format!("環境變數：{xai_env}"));
                ui.add_space(4.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.xai_key_edit)
                        .password(!self.show_api_keys)
                        .hint_text("貼上 xAI API Key")
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal(|ui| {
                    if crate::theme::primary_button(ui, "儲存 xAI Key").clicked() {
                        key_action = Some(KeyAction::Save(ProviderKey::Xai));
                    }
                    if crate::theme::secondary_button(ui, "清除").clicked()
                        && xai_configured
                    {
                        key_action = Some(KeyAction::Clear(ProviderKey::Xai));
                    }
                });
                crate::theme::card_end(ui);

                ui.add_space(8.0);
                // OpenRouter card
                crate::theme::card_begin(ui, None);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("OpenRouter").size(17.0).color(crate::theme::colors::TEXT_PRIMARY).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (label, color) = configured_badge(openrouter_configured);
                        ui.colored_label(color, label);
                    });
                });
                crate::theme::caption(ui, &format!("環境變數：{openrouter_env}"));
                ui.add_space(4.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.openrouter_key_edit)
                        .password(!self.show_api_keys)
                        .hint_text("貼上 OpenRouter API Key")
                        .desired_width(f32::INFINITY),
                );
                ui.horizontal(|ui| {
                    if crate::theme::primary_button(ui, "儲存 OpenRouter Key").clicked() {
                        key_action = Some(KeyAction::Save(ProviderKey::OpenRouter));
                    }
                    if crate::theme::secondary_button(ui, "清除").clicked()
                        && openrouter_configured
                    {
                        key_action = Some(KeyAction::Clear(ProviderKey::OpenRouter));
                    }
                });
                crate::theme::card_end(ui);

                ui.add_space(10.0);
                ui.checkbox(&mut self.show_api_keys, "顯示輸入中的 API Key");

                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("API Key 環境變數名稱設定")
                        .size(15.0)
                        .strong(),
                );
                ui.label(
                    egui::RichText::new(
                        "變更後需要點擊下方「儲存設定」才會生效。",
                    )
                    .small()
                    .color(egui::Color32::from_rgb(110, 110, 115)),
                );
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("OpenAI")
                            .small()
                            .color(egui::Color32::from_rgb(110, 110, 115)),
                    );
                    ui.add_sized(
                        egui::vec2(180.0, 18.0),
                        egui::TextEdit::singleline(
                            &mut self.app.config.openai.api_key_env,
                        )
                        .font(egui::TextStyle::Small)
                        .hint_text("OPENAI_API_KEY"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("xAI")
                            .small()
                            .color(egui::Color32::from_rgb(110, 110, 115)),
                    );
                    ui.add_sized(
                        egui::vec2(180.0, 18.0),
                        egui::TextEdit::singleline(
                            &mut self.app.config.xai.api_key_env,
                        )
                        .font(egui::TextStyle::Small)
                        .hint_text("XAI_API_KEY"),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("OpenRouter")
                            .small()
                            .color(egui::Color32::from_rgb(110, 110, 115)),
                    );
                    ui.add_sized(
                        egui::vec2(180.0, 18.0),
                        egui::TextEdit::singleline(
                            &mut self.app.config.openrouter.api_key_env,
                        )
                        .font(egui::TextStyle::Small)
                        .hint_text("OPENROUTER_API_KEY"),
                    );
                });

                if let Some(warning) = &self.startup_warning {
                    ui.colored_label(
                        crate::theme::colors::ORANGE_WARNING,
                        format!("啟動提醒：{warning}"),
                    );
                }
                if let Some(message) = &self.key_message {
                    let color = if message.success {
                        crate::theme::colors::GREEN_SUCCESS
                    } else {
                        crate::theme::colors::RED_ERROR
                    };
                    ui.colored_label(color, &message.text);
                }

                ui.add_space(16.0);
                if crate::theme::primary_button(ui, "儲存設定").clicked() {
                    config_save = true;
                }
                }); // ScrollArea
            });

        self.settings_window_open = open;
        if let Some(action) = key_action {
            self.apply_key_action(action);
        }
        if config_save {
            self.app.save_settings();
        }
    }

    fn apply_key_action(&mut self, action: KeyAction) {
        let (provider_name, env_name, key_value) = match action {
            KeyAction::Save(ProviderKey::OpenAi) => (
                "OpenAI",
                self.app.config.openai.api_key_env.clone(),
                Some(self.openai_key_edit.trim().to_string()),
            ),
            KeyAction::Save(ProviderKey::Xai) => (
                "xAI",
                self.app.config.xai.api_key_env.clone(),
                Some(self.xai_key_edit.trim().to_string()),
            ),
            KeyAction::Save(ProviderKey::OpenRouter) => (
                "OpenRouter",
                self.app.config.openrouter.api_key_env.clone(),
                Some(self.openrouter_key_edit.trim().to_string()),
            ),
            KeyAction::Clear(ProviderKey::OpenAi) => {
                ("OpenAI", self.app.config.openai.api_key_env.clone(), None)
            }
            KeyAction::Clear(ProviderKey::Xai) => {
                ("xAI", self.app.config.xai.api_key_env.clone(), None)
            }
            KeyAction::Clear(ProviderKey::OpenRouter) => (
                "OpenRouter",
                self.app.config.openrouter.api_key_env.clone(),
                None,
            ),
        };

        // Validate that the env var name is a syntactically valid environment
        // variable name before touching Credential Manager.  This mirrors the
        // check in AppConfig::validate() so the user cannot accidentally save
        // credentials under a name that would be rejected at startup.
        if !crate::config::is_environment_variable_name(&env_name) {
            self.key_message = Some(KeyMessage {
                success: false,
                text: format!(
                    "{provider_name} 環境變數名稱「{env_name}」不合法，必須以字母或底線開頭且僅含字母、數字與底線。請修正後點擊下方「儲存設定」。"
                ),
            });
            return;
        }

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
        self.poll_model_fetch();
        eframe::App::update(&mut self.app, ctx, frame);
        self.show_settings_launcher(ctx);
        self.show_update_launcher(ctx);
        self.show_settings_window(ctx);
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

/// Decide whether the backup repaint timer should be armed.
///
/// The backup keeps the egui event loop alive for two critical states:
///   * `window_hidden` — after the user hides to tray (window is
///     moved off-screen with WS_EX_TOOLWINDOW).  A repaint_after timer
///     is kept so the tray channel is polled even if a tray callback's
///     request_repaint() does not fully propagate through winit.
///   * `exit_requested` — `request_exit` has sent `Close` but the
///     event has not yet been delivered. Stopping repaint here could
///     stall the exit indefinitely.
///
/// Once `CloseDecision::Exit` is taken and the native close proceeds,
/// the eframe App is dropped on that same frame, so any outstanding
/// `repaint_after` is harmless.
fn should_backup_repaint(window_hidden: bool, exit_requested: bool) -> bool {
    window_hidden || exit_requested
}

fn append_warning(current: &mut Option<String>, warning: &str) {
    if let Some(current) = current {
        current.push('\n');
        current.push_str(warning);
    } else {
        *current = Some(warning.to_string());
    }
}

/// Apply `WS_EX_TOOLWINDOW` and remove `WS_EX_APPWINDOW` so the window loses
/// its taskbar entry. Safe to call multiple times.
#[cfg(target_os = "windows")]
unsafe fn window_hide_ext_style() {
    if let Some(hwnd) = unsafe { find_main_hwnd() } {
        // SAFETY: hwnd is the valid main window HWND obtained via
        // EnumWindows + PID/title verification above.
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                (style & !(WS_EX_APPWINDOW as isize)) | WS_EX_TOOLWINDOW as isize,
            );
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_NOACTIVATE,
            );
        }
    }
}

/// Remove `WS_EX_TOOLWINDOW` (and ensure `WS_EX_APPWINDOW`) so the taskbar
/// button reappears.  Safe to call multiple times.
#[cfg(target_os = "windows")]
unsafe fn window_show_ext_style() {
    if let Some(hwnd) = unsafe { find_main_hwnd() } {
        // SAFETY: hwnd is the valid main window HWND obtained via
        // EnumWindows + PID/title verification above.
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                (style & !(WS_EX_TOOLWINDOW as isize)) | WS_EX_APPWINDOW as isize,
            );
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_NOACTIVATE,
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayAction {
    Show,
    Exit,
}

#[cfg(target_os = "windows")]
struct SystemTray {
    _icon: tray_icon::TrayIcon,
    pending: std::sync::mpsc::Receiver<TrayAction>,
}

#[cfg(target_os = "windows")]
impl SystemTray {
    fn new(ctx: &egui::Context) -> Result<Self, String> {
        use tray_icon::menu::{Menu, MenuEvent, MenuItem};
        use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

        let show = MenuItem::new("顯示 SpeakType Cloud", true, None);
        let exit = MenuItem::new("退出", true, None);
        let show_id = show.id().clone();
        let exit_id = exit.id().clone();
        let menu = Menu::with_items(&[&show, &exit]).map_err(|error| error.to_string())?;
        let icon = tray_icon_image()?;
        // Left-click restores the window; right-click opens the context menu.
        let tray = TrayIconBuilder::new()
            .with_tooltip("SpeakType Cloud")
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|error| error.to_string())?;
        let tray_id = tray.id().clone();

        let (tx, rx) = std::sync::mpsc::channel();
        // tray-icon/muda deliver events on the Win32 message pump thread. With
        // eframe/winit those messages do not automatically schedule an egui
        // frame, so a hidden window would never poll the default receivers.
        // Forward into our channel and wake egui on every tray/menu event.
        let menu_tx = tx.clone();
        let menu_ctx = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if event.id == exit_id {
                let _ = menu_tx.send(TrayAction::Exit);
            } else if event.id == show_id {
                let _ = menu_tx.send(TrayAction::Show);
            }
            menu_ctx.request_repaint();
        }));

        let tray_tx = tx;
        let tray_ctx = ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            let show = match &event {
                TrayIconEvent::DoubleClick {
                    id,
                    button: MouseButton::Left,
                    ..
                } => id == &tray_id,
                TrayIconEvent::Click {
                    id,
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => id == &tray_id,
                _ => false,
            };
            if show {
                let _ = tray_tx.send(TrayAction::Show);
            }
            tray_ctx.request_repaint();
        }));

        Ok(Self {
            _icon: tray,
            pending: rx,
        })
    }

    fn poll_action(&self) -> Option<TrayAction> {
        drain_tray_actions(self.pending.try_iter())
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
    fn new(_ctx: &egui::Context) -> Result<Self, String> {
        Err("系統匣目前僅支援 Windows".to_string())
    }

    fn poll_action(&self) -> Option<TrayAction> {
        None
    }
}

/// Collapse a burst of tray events into a single action.
/// Exit always wins so a Show click cannot cancel an explicit quit.
fn drain_tray_actions<I>(actions: I) -> Option<TrayAction>
where
    I: IntoIterator<Item = TrayAction>,
{
    let mut action = None;
    for next in actions {
        if matches!(next, TrayAction::Exit) {
            return Some(TrayAction::Exit);
        }
        action = Some(next);
    }
    action
}

fn configured_badge(configured: bool) -> (&'static str, egui::Color32) {
    if configured {
        ("● 已設定", crate::theme::colors::GREEN_SUCCESS)
    } else {
        ("○ 尚未設定", crate::theme::colors::TEXT_SECONDARY)
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
    fn close_decision_respects_exit_requested_with_tray() {
        // When exit is explicitly requested, close should proceed even with tray.
        assert_eq!(close_decision(true, true), CloseDecision::Exit);
    }

    #[test]
    fn close_decision_hides_when_no_exit_and_tray_available() {
        assert_eq!(close_decision(true, false), CloseDecision::Hide);
    }

    #[test]
    fn drain_tray_actions_prefers_exit_over_show() {
        assert_eq!(
            drain_tray_actions([TrayAction::Show, TrayAction::Exit, TrayAction::Show]),
            Some(TrayAction::Exit)
        );
        assert_eq!(
            drain_tray_actions([TrayAction::Show, TrayAction::Show]),
            Some(TrayAction::Show)
        );
        assert_eq!(drain_tray_actions(std::iter::empty()), None);
    }

    #[test]
    fn key_action_enum_is_constructable() {
        // Ensure KeyAction/ProviderKey enums compile and match.
        let actions = [
            KeyAction::Save(ProviderKey::OpenAi),
            KeyAction::Save(ProviderKey::Xai),
            KeyAction::Save(ProviderKey::OpenRouter),
            KeyAction::Clear(ProviderKey::OpenAi),
            KeyAction::Clear(ProviderKey::Xai),
            KeyAction::Clear(ProviderKey::OpenRouter),
        ];
        assert_eq!(actions.len(), 6);
    }

    #[test]
    fn backup_repaint_covers_hidden_and_pending_exit() {
        // should_backup_repaint must return true when the window is hidden
        // (tray polling) OR when exit is pending (between request_exit and
        // close_requested), and only false when neither condition applies.
        //
        // Truth table:
        //   hidden=false, exit=false  → false (visible, no exit)
        //   hidden=true,  exit=false  → true  (tray mode — keep polling)
        //   hidden=false, exit=true   → true  (pending exit — deliver Close)
        //   hidden=true,  exit=true   → true  (both — keep repainting)
        assert!(!should_backup_repaint(false, false));
        assert!(should_backup_repaint(true, false));
        assert!(should_backup_repaint(false, true));
        assert!(should_backup_repaint(true, true));
    }

    #[test]
    fn exit_in_drain_actions_always_takes_priority() {
        // Verify that drain_tray_actions returns Exit even when
        // interspersed with multiple Show events.
        let actions = vec![TrayAction::Show, TrayAction::Show, TrayAction::Exit];
        assert_eq!(drain_tray_actions(actions), Some(TrayAction::Exit));

        // A single Exit also works.
        assert_eq!(
            drain_tray_actions(vec![TrayAction::Exit]),
            Some(TrayAction::Exit)
        );
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
