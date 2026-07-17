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
mod secrets;
mod shell;
mod theme;
mod transcription;

use shell::AppleShell;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SpeakType Cloud")
            .with_inner_size([780.0, 680.0])
            .with_min_inner_size([680.0, 560.0]),
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
