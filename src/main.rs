#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod config;
mod error;
mod fonts;
mod history;
mod hotkey;
mod injector;
mod paths;
mod postprocess;
mod providers;
mod realtime;
mod realtime_worker;
mod secrets;
mod shell;
mod startup;
mod theme;
mod transcription;
mod updater;
mod vad;

use shell::AppleShell;

fn main() -> eframe::Result<()> {
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
