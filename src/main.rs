#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod config;
mod error;
mod history;
mod hotkey;
mod injector;
mod paths;
mod postprocess;
mod providers;
mod transcription;

use app::SpeakTypeCloudApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SpeakType Cloud")
            .with_inner_size([720.0, 620.0])
            .with_min_inner_size([620.0, 520.0]),
        ..Default::default()
    };

    eframe::run_native(
        "SpeakType Cloud",
        options,
        Box::new(|cc| Box::new(SpeakTypeCloudApp::new(cc))),
    )
}
