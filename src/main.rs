#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod cleanup;
mod config;
mod error;
#[cfg(all(feature = "legacy-ui", not(feature = "slint-ui")))]
mod fonts;
mod history;
mod hotkey;
mod injector;
mod paths;
mod postprocess;
mod preview;
mod providers;
mod realtime;
mod realtime_worker;
mod runtime;
mod secrets;
#[cfg(all(feature = "legacy-ui", not(feature = "slint-ui")))]
mod shell;
mod single_instance;
#[cfg(feature = "slint-ui")]
mod slint_ui;
mod startup;
#[cfg(all(feature = "legacy-ui", not(feature = "slint-ui")))]
mod theme;
mod transcription;
mod updater;
mod vad;
mod win_util;

#[cfg(all(feature = "legacy-ui", not(feature = "slint-ui")))]
use shell::AppleShell;
#[cfg(all(feature = "legacy-ui", not(feature = "slint-ui")))]
use single_instance::SingleInstance;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse before acquiring a mutex or reading the user's profile.
    let mode = runtime::RuntimeMode::parse(std::env::args().skip(1))?;
    #[cfg(feature = "slint-ui")]
    {
        slint_ui::run(mode)
    }
    #[cfg(all(feature = "legacy-ui", not(feature = "slint-ui")))]
    {
        if mode.is_demo() {
            return Err("Demo requires the Slint interface".into());
        }
        legacy_main().map_err(|e| e.to_string().into())
    }
    #[cfg(not(any(feature = "legacy-ui", feature = "slint-ui")))]
    {
        let _ = mode;
        Err("Build requires a UI feature".into())
    }
}

#[cfg(all(feature = "legacy-ui", not(feature = "slint-ui")))]
fn legacy_main() -> eframe::Result<()> {
    let _single_instance = match SingleInstance::acquire() {
        Ok(instance) => Some(instance),
        Err(error) => {
            eprintln!("{error}");
            return Ok(());
        }
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SpeakType Cloud")
            .with_inner_size(theme::DEFAULT_WINDOW_SIZE)
            .with_min_inner_size(theme::MIN_WINDOW_SIZE),
        ..Default::default()
    };

    eframe::run_native(
        "SpeakType Cloud",
        options,
        Box::new(|cc| {
            if let Err(error) = fonts::install_cjk_font(&cc.egui_ctx) {
                eprintln!("{error}");
            }
            Box::new(AppleShell::new(cc))
        }),
    )
}
