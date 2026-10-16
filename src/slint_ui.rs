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

pub fn run(mode: RuntimeMode) -> Result<(), Box<dyn std::error::Error>> {
    if !mode.is_demo() {
        return Err("Slint migration preview currently supports --demo only".into());
    }
    let ui = MainWindow::new()?;
    let tray = DemoTray::new();
    let tray_available = tray.is_ok();
    ui.set_tray_available(tray_available);
    let tray = tray.ok();
    let app = Rc::new(RefCell::new(SpeakTypeCloudApp::new(mode)));
    ui.set_demo(true);
    ui.set_status("就緒。按「開始示範」測試完整的離線工作流程。".into());
    let controller = app.clone();
    ui.on_toggle_recording(move || controller.borrow_mut().toggle());
    let controller = app.clone();
    ui.on_cancel(move || controller.borrow_mut().cancel());
    let controller = app.clone();
    ui.on_retry(move || controller.borrow_mut().retry());
    let controller = app.clone();
    ui.on_copy_preview(move |text| controller.borrow_mut().copy_preview(text.to_string()));
    let controller = app.clone();
    ui.on_paste_preview(move |text| controller.borrow_mut().paste_preview(text.to_string()));
    let controller = app.clone();
    ui.on_exit_app(move || {
        controller.borrow_mut().shutdown();
        let _ = slint::quit_event_loop();
    });
    let weak = ui.as_weak();
    ui.on_hide_window(move || {
        if tray_available {
            if let Some(ui) = weak.upgrade() {
                let _ = ui.hide();
            }
        }
    });
    let controller = app.clone();
    ui.window().on_close_requested(move || {
        if !tray_available {
            controller.borrow_mut().shutdown();
            let _ = slint::quit_event_loop();
        }
        slint::CloseRequestResponse::HideWindow
    });
    let weak = ui.as_weak();
    let controller = app.clone();
    let timer = slint::Timer::default();
    let mut last_text = String::new();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(40),
        move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let mut app = controller.borrow_mut();
            if let Some(tray) = &tray {
                match tray.poll() {
                    Some(TrayAction::Show) => {
                        let _ = ui.show();
                    }
                    Some(TrayAction::Exit) => {
                        app.shutdown();
                        let _ = slint::quit_event_loop();
                    }
                    None => {}
                }
            }
            app.tick();
            ui.set_recording(app.is_recording());
            ui.set_busy(app.is_busy());
            ui.set_status(app.status().into());
            ui.set_error(app.last_error.as_deref().unwrap_or_default().into());
            ui.set_raw_text(app.raw_text().into());
            ui.set_partial_text(app.partial_text().into());
            // Preserve edits while no new completion arrives.
            if app.text() != last_text {
                last_text = app.text().to_string();
                ui.set_final_text(last_text.as_str().into());
            }
        },
    );
    ui.show()?;
    slint::run_event_loop_until_quit()?;
    timer.stop();
    app.borrow_mut().shutdown();
    Ok(())
}

enum TrayAction {
    Show,
    Exit,
}

#[cfg(windows)]
struct DemoTray {
    icon: tray_icon::TrayIcon,
    show: tray_icon::menu::MenuId,
    exit: tray_icon::menu::MenuId,
}

#[cfg(windows)]
impl DemoTray {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        use tray_icon::menu::{Menu, MenuItem};
        let show = MenuItem::new("顯示離線示範", true, None);
        let exit = MenuItem::new("退出離線示範", true, None);
        let menu = Menu::with_items(&[&show, &exit])?;
        // The same bundled SVG is rasterized; Windows receives RGBA pixels.
        let image = slint::Image::load_from_svg_data(include_bytes!("../ui/icons/microphone.svg"))?;
        let rgba = image.to_rgba8().ok_or("無法建立系統匣圖示")?;
        let icon =
            tray_icon::Icon::from_rgba(rgba.as_bytes().to_vec(), rgba.width(), rgba.height())?;
        let icon = tray_icon::TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip("SpeakType Cloud · 離線示範")
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
struct DemoTray;
#[cfg(not(windows))]
impl DemoTray {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Err("Windows tray only".into())
    }
    fn poll(&self) -> Option<TrayAction> {
        None
    }
}
