use std::path::PathBuf;
use std::process::ExitCode;

use eframe::egui;
use wordstrike::native_ui::NativeEditor;

fn main() -> ExitCode {
    let mut args = std::env::args_os().skip(1);
    let Some(path) = args.next().map(PathBuf::from) else {
        eprintln!("Usage: wordstrike-native FILE.wstrike [--capture FILE.png]");
        return ExitCode::FAILURE;
    };
    let capture_path = match (args.next(), args.next(), args.next()) {
        (None, None, None) => None,
        (Some(flag), Some(output), None) if flag == "--capture" => {
            let output = PathBuf::from(output);
            if output.extension().is_none_or(|ext| ext != "png") {
                eprintln!("Capture path must end in .png");
                return ExitCode::FAILURE;
            }
            Some(output)
        }
        _ => {
            eprintln!("Usage: wordstrike-native FILE.wstrike [--capture FILE.png]");
            return ExitCode::FAILURE;
        }
    };
    let mut editor = match NativeEditor::open_or_create(path) {
        Ok(editor) => editor,
        Err(error) => {
            eprintln!("WordStrike: {error}");
            return ExitCode::FAILURE;
        }
    };
    let capture_result = capture_path.map(|path| editor.capture_once(path));
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
        Ok(()) => {
            if let Some(result) = capture_result {
                match result.lock().unwrap().take() {
                    Some(Ok(())) => ExitCode::SUCCESS,
                    Some(Err(error)) => {
                        eprintln!("WordStrike capture: {error}");
                        ExitCode::FAILURE
                    }
                    None => {
                        eprintln!("WordStrike capture: window closed before screenshot completed");
                        ExitCode::FAILURE
                    }
                }
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("WordStrike window: {error}");
            ExitCode::FAILURE
        }
    }
}
