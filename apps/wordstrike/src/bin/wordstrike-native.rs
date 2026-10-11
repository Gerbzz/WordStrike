use std::path::PathBuf;
use std::process::ExitCode;

use eframe::egui;
use wordstrike::native_ui::NativeEditor;

fn main() -> ExitCode {
    let Some(path) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("Usage: wordstrike-native FILE.wstrike");
        return ExitCode::FAILURE;
    };
    let editor = match NativeEditor::open_or_create(path) {
        Ok(editor) => editor,
        Err(error) => {
            eprintln!("WordStrike: {error}");
            return ExitCode::FAILURE;
        }
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("WordStrike")
            .with_inner_size([1380.0, 840.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    match eframe::run_native(
        "WordStrike",
        options,
        Box::new(move |_cc| Ok(Box::new(editor))),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("WordStrike window: {error}");
            ExitCode::FAILURE
        }
    }
}
