use crate::app::SpeakTypeCloudApp;
use crate::runtime::RuntimeMode;
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

// Slint's generated code uses unwrap internally. Keep the exception scoped to
// generated code, while all handwritten controller/adapter code stays linted.
#[allow(clippy::unwrap_used)]
mod generated {
    slint::include_modules!();
}
use generated::MainWindow;
#[path = "slint_services.rs"]
mod services;
#[path = "slint_settings.rs"]
mod settings;
use slint::{ModelRc, SharedString, VecModel};
fn string_model(values: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    Rc::new(VecModel::from(
        values
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    ))
    .into()
}

pub fn run(mode: RuntimeMode) -> Result<(), Box<dyn std::error::Error>> {
    let _instance = if mode.is_demo() {
        None
    } else {
        Some(crate::single_instance::SingleInstance::acquire()?)
    };
    let ui = MainWindow::new()?;
    let tray_result = SystemTray::new(mode);
    ui.set_tray_available(tray_result.is_ok());
    let tray_available = tray_result.is_ok();
    if mode.is_lifecycle_diagnostic() && !tray_available {
        return Err("Lifecycle diagnostic requires a working tray".into());
    }
    let tray = tray_result.ok();
    let mut core = SpeakTypeCloudApp::new(mode);
    if !mode.is_demo() {
        if let Err(error) = crate::secrets::hydrate_process_environment(&core.config) {
            core.last_error = Some(error.to_string());
        }
    }
    ui.set_settings(settings::from_config(&core.config, &core.devices));
    ui.set_devices(string_model(
        std::iter::once("系統預設麥克風".into()).chain(core.devices.clone()),
    ));
    ui.set_demo(mode.is_demo());
    ui.set_lifecycle_diagnostic(mode.is_lifecycle_diagnostic());
    ui.set_version(env!("CARGO_PKG_VERSION").into());
    let app = Rc::new(RefCell::new(core));
    let services = Rc::new(RefCell::new(services::Services::new(mode)));
    macro_rules! app_action {
        ($callback:ident,$method:ident) => {{
            let controller = app.clone();
            ui.$callback(move || controller.borrow_mut().$method());
        }};
    }
    app_action!(on_toggle_recording, toggle);
    app_action!(on_cancel, cancel);
    app_action!(on_retry, retry);
    app_action!(on_restore_original, restore_original);
    app_action!(on_batch_fallback, confirm_batch_fallback);
    let controller = app.clone();
    ui.on_copy_preview(move |text| controller.borrow_mut().copy_preview(text.to_string()));
    let controller = app.clone();
    ui.on_paste_preview(move |text| controller.borrow_mut().paste_preview(text.to_string()));
    let controller = app.clone();
    let weak = ui.as_weak();
    ui.on_save_settings(move || {
        if let Some(ui) = weak.upgrade() {
            let mut app = controller.borrow_mut();
            match settings::apply(&ui.get_settings(), &app.config, &app.devices) {
                Ok(config) => {
                    app.hotkey_edit = config.hotkey.clone();
                    app.keyterms_edit = config.xai.keyterms.join("\n");
                    app.config = config;
                    app.save_settings();
                }
                Err(error) => app.last_error = Some(error.to_string()),
            }
        }
    });
    let controller = app.clone();
    let weak = ui.as_weak();
    ui.on_reset_settings(move || {
        if let Some(ui) = weak.upgrade() {
            let app = controller.borrow();
            ui.set_settings(settings::from_config(&app.config, &app.devices));
        }
    });
    let controller = app.clone();
    let service = services.clone();
    ui.on_save_key(move |index, key| {
        service.borrow_mut().key_action(
            controller.borrow().config.clone(),
            settings::provider(index),
            Some(key.to_string()),
        )
    });
    let controller = app.clone();
    let service = services.clone();
    ui.on_clear_key(move |index| {
        service.borrow_mut().key_action(
            controller.borrow().config.clone(),
            settings::provider(index),
            None,
        )
    });
    let controller = app.clone();
    let service = services.clone();
    ui.on_fetch_models(move |index| {
        service.borrow_mut().fetch_models(
            controller.borrow().config.clone(),
            settings::provider(index),
        )
    });
    let service = services.clone();
    let weak = ui.as_weak();
    ui.on_pick_model(move |index, model| {
        if service.borrow().models_provider != Some(settings::provider(index)) {
            return;
        }
        if let Some(ui) = weak.upgrade() {
            let mut data = ui.get_settings();
            match index {
                0 => data.openai_model = model,
                2 => data.openrouter_model = model,
                3 => data.gemini_model = model,
                _ => {}
            }
            ui.set_settings(data);
        }
    });
    macro_rules! service_action {
        ($callback:ident,$method:ident) => {{
            let service = services.clone();
            ui.$callback(move || service.borrow_mut().$method());
        }};
    }
    service_action!(on_load_history, load_history);
    service_action!(on_check_update, check_update);
    service_action!(on_download_update, download_update);
    service_action!(on_install_update, install_update);
    service_action!(on_stop_playback, stop_playback);
    let service = services.clone();
    ui.on_delete_history(move |id| service.borrow_mut().delete_history(id.to_string()));
    let service = services.clone();
    ui.on_play_history(move |id| service.borrow_mut().play_history(&id));
    let controller = app.clone();
    let service = services.clone();
    ui.on_cleanup_history(move || {
        service
            .borrow_mut()
            .cleanup_history(controller.borrow().config.history.retention_days)
    });
    let controller = app.clone();
    let service = services.clone();
    ui.on_open_history(move |id| {
        if let Some(entry) = service
            .borrow()
            .history
            .iter()
            .find(|e| e.id == id.as_str())
        {
            controller
                .borrow_mut()
                .open_history_preview(entry.text.clone());
        }
    });
    let controller = app.clone();
    let weak = ui.as_weak();
    ui.on_accept_term(move |index| {
        let mut app = controller.borrow_mut();
        app.accept_dictionary_suggestion(index.max(0) as usize);
        if let Some(ui) = weak.upgrade() {
            ui.set_settings(settings::from_config(&app.config, &app.devices));
        }
    });
    let controller = app.clone();
    ui.on_reject_term(move |index| {
        controller
            .borrow_mut()
            .reject_dictionary_suggestion(index.max(0) as usize)
    });
    let controller = app.clone();
    let weak = ui.as_weak();
    ui.on_accept_style(move || {
        let mut app = controller.borrow_mut();
        app.accept_style_suggestion();
        if let Some(ui) = weak.upgrade() {
            ui.set_settings(settings::from_config(&app.config, &app.devices));
        }
    });
    let controller = app.clone();
    ui.on_reject_style(move || controller.borrow_mut().style_suggestion = None);
    let controller = app.clone();
    let service = services.clone();
    let weak = ui.as_weak();
    ui.on_exit_app(move || {
        if let Some(ui) = weak.upgrade() {
            apply_window_action(
                TrayAction::Exit,
                &ui,
                &mut controller.borrow_mut(),
                &mut service.borrow_mut(),
            );
        }
    });
    let weak = ui.as_weak();
    let controller = app.clone();
    let service = services.clone();
    ui.on_hide_window(move || {
        if tray_available {
            if let Some(ui) = weak.upgrade() {
                apply_window_action(
                    TrayAction::Hide,
                    &ui,
                    &mut controller.borrow_mut(),
                    &mut service.borrow_mut(),
                );
            }
        }
    });
    let controller = app.clone();
    let service = services.clone();
    ui.window().on_close_requested(move || {
        if !tray_available {
            service.borrow_mut().shutdown();
            controller.borrow_mut().shutdown();
        }
        slint::CloseRequestResponse::HideWindow
    });
    let weak = ui.as_weak();
    let controller = app.clone();
    let service = services.clone();
    let timer = slint::Timer::default();
    let mut projection = crate::preview::PreviewProjection::default();
    let mut service_revision = u64::MAX;
    let mut last_suggestions = Vec::new();
    let diagnostic_started = std::time::Instant::now();
    let mut diagnostic_step = 0;
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(40),
        move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            // Explicit offline diagnostic: these are the real callbacks/Slint
            // lifecycle, not mouse events and not evidence of a tray click.
            let diagnostic_action = if mode.is_lifecycle_diagnostic() {
                let due = [10, 25, 40, 55, 70];
                if diagnostic_step < due.len()
                    && diagnostic_started.elapsed().as_secs() >= due[diagnostic_step]
                {
                    diagnostic_step += 1;
                    match diagnostic_step {
                        1 => {
                            println!("LIFECYCLE hide-callback");
                            ui.invoke_hide_window();
                            None
                        }
                        2 | 4 => {
                            println!("LIFECYCLE shared-tray-show");
                            Some(TrayAction::Show)
                        }
                        3 => {
                            println!("LIFECYCLE slint-close-event");
                            if let Err(error) = ui.window().dispatch_event_with_result(
                                slint::platform::WindowEvent::CloseRequested,
                            ) {
                                eprintln!("LIFECYCLE close-error: {error}");
                            }
                            None
                        }
                        _ => {
                            println!("LIFECYCLE shared-tray-exit");
                            Some(TrayAction::Exit)
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };
            let mut app = controller.borrow_mut();
            let mut service = service.borrow_mut();
            if let Some(tray) = &tray {
                if let Some(action) = tray.poll() {
                    apply_window_action(action, &ui, &mut app, &mut service);
                }
            }
            if let Some(action) = diagnostic_action {
                apply_window_action(action, &ui, &mut app, &mut service);
            }
            app.tick();
            service.poll();
            ui.set_closing(app.is_closing());
            if app.shutdown_ready() && !service.busy() {
                let _ = slint::quit_event_loop();
                return;
            }
            ui.set_recording(app.is_recording());
            ui.set_capture_health(app.capture_health().into());
            ui.set_busy(app.is_busy());
            ui.set_status(app.status().into());
            ui.set_error(app.last_error.as_deref().unwrap_or_default().into());
            ui.set_raw_text(app.raw_text().into());
            ui.set_partial_text(app.partial_text().into());
            ui.set_fallback_available(app.fallback_available());
            if let Some(text) = projection.project(app.preview_revision(), app.text()) {
                ui.set_final_text(text.into());
            }
            let suggestions: Vec<_> = app
                .dictionary_suggestions
                .iter()
                .map(|e| (e.source.clone(), e.replacement.clone()))
                .collect();
            if suggestions != last_suggestions {
                last_suggestions = suggestions;
                ui.set_suggestions(
                    Rc::new(VecModel::from(
                        app.dictionary_suggestions
                            .iter()
                            .map(|e| generated::SuggestionRow {
                                source: e.source.as_str().into(),
                                replacement: e.replacement.as_str().into(),
                            })
                            .collect::<Vec<_>>(),
                    ))
                    .into(),
                );
            }
            ui.set_style_suggestion(app.style_suggestion.as_deref().unwrap_or_default().into());
            ui.set_service_busy(service.busy());
            ui.set_service_status(service.notice.as_str().into());
            ui.set_update_status(service.update_status.as_str().into());
            ui.set_update_download_ready(service.can_download());
            ui.set_update_install_ready(service.can_install());
            if service_revision != service.revision {
                service_revision = service.revision;
                ui.set_history(
                    Rc::new(VecModel::from(
                        service
                            .history
                            .iter()
                            .rev()
                            .map(|e| generated::HistoryRow {
                                id: e.id.as_str().into(),
                                stamp: format!(
                                    "{} · {} · {:.1} 秒",
                                    e.created_at.format("%Y-%m-%d %H:%M"),
                                    e.provider,
                                    e.duration_secs
                                )
                                .into(),
                                text: e.preview().into(),
                                audio: e.audio.is_some(),
                            })
                            .collect::<Vec<_>>(),
                    ))
                    .into(),
                );
                ui.set_models(string_model(service.models.clone()));
                ui.set_key_status(if mode.is_demo() {
                    "離線示範：不讀取金鑰狀態".into()
                } else {
                    [
                        crate::config::ProviderKind::OpenAi,
                        crate::config::ProviderKind::Xai,
                        crate::config::ProviderKind::OpenRouter,
                        crate::config::ProviderKind::Gemini,
                    ]
                    .iter()
                    .map(|provider| {
                        let mut c = app.config.clone();
                        c.provider = *provider;
                        format!(
                            "{}：{}",
                            provider.label(),
                            if crate::secrets::is_api_key_configured(c.api_key_env()) {
                                "已設定"
                            } else {
                                "未設定"
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("　")
                    .into()
                });
            }
        },
    );
    ui.show()?;
    slint::run_event_loop_until_quit()?;
    timer.stop();
    services.borrow().stop_playback();
    app.borrow_mut().shutdown();
    Ok(())
}

enum TrayAction {
    Hide,
    Show,
    Exit,
}

fn apply_window_action(
    action: TrayAction,
    ui: &MainWindow,
    app: &mut SpeakTypeCloudApp,
    service: &mut services::Services,
) {
    let result = match action {
        TrayAction::Hide => ui.hide(),
        TrayAction::Show | TrayAction::Exit => {
            if matches!(action, TrayAction::Exit) {
                service.shutdown();
                app.shutdown();
            }
            // Never restore only the native HWND: Slint's visibility state and
            // renderer must be reactivated together. Also restore a minimized
            // window and request a fresh frame after remapping its surface.
            ui.show().map(|()| {
                ui.window().set_minimized(false);
                ui.window().request_redraw();
            })
        }
    };
    if let Err(error) = result {
        app.last_error = Some(format!("視窗操作失敗：{error}"));
    }
}

#[cfg(windows)]
struct SystemTray {
    icon: tray_icon::TrayIcon,
    show: tray_icon::menu::MenuId,
    exit: tray_icon::menu::MenuId,
}

#[cfg(windows)]
impl SystemTray {
    fn new(mode: RuntimeMode) -> Result<Self, Box<dyn std::error::Error>> {
        use tray_icon::menu::{Menu, MenuItem};
        let show = MenuItem::new("顯示 SpeakType Cloud", true, None);
        let exit = MenuItem::new("退出 SpeakType Cloud", true, None);
        let menu = Menu::with_items(&[&show, &exit])?;
        // The same bundled SVG is rasterized; Windows receives RGBA pixels.
        let image = slint::Image::load_from_svg_data(include_bytes!("../ui/icons/microphone.svg"))?;
        let rgba = image.to_rgba8().ok_or("無法建立系統匣圖示")?;
        let icon =
            tray_icon::Icon::from_rgba(rgba.as_bytes().to_vec(), rgba.width(), rgba.height())?;
        let icon = tray_icon::TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip(if mode.is_demo() {
                "SpeakType Cloud · 離線示範"
            } else {
                "SpeakType Cloud"
            })
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()?;
        Ok(Self {
            icon,
            show: show.id().clone(),
            exit: exit.id().clone(),
        })
    }

    fn poll(&self) -> Option<TrayAction> {
        use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
        let mut action = None;
        for event in tray_icon::menu::MenuEvent::receiver().try_iter() {
            if event.id == self.exit {
                return Some(TrayAction::Exit);
            }
            if event.id == self.show {
                action = Some(TrayAction::Show);
            }
        }
        for event in TrayIconEvent::receiver().try_iter() {
            if matches!(event, TrayIconEvent::Click { ref id, button: MouseButton::Left, button_state: MouseButtonState::Up, .. } if id == self.icon.id())
            {
                action = Some(TrayAction::Show);
            }
        }
        action
    }
}

#[cfg(not(windows))]
struct SystemTray;
#[cfg(not(windows))]
impl SystemTray {
    fn new(mode: RuntimeMode) -> Result<Self, Box<dyn std::error::Error>> {
        Err("Windows tray only".into())
    }
    fn poll(&self) -> Option<TrayAction> {
        None
    }
}
