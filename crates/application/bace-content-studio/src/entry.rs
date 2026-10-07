use std::process::ExitCode;

pub fn run() -> ExitCode {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1360.0, 900.0])
            .with_min_inner_size([1060.0, 700.0]),
        ..Default::default()
    };
    match eframe::run_native(
        "BetterACE Content Studio",
        options,
        Box::new(|cc| {
            crate::theme::apply(&cc.egui_ctx);
            Ok(Box::new(crate::app::Studio::default()))
        }),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Could not open BetterACE Content Studio: {error}");
            ExitCode::FAILURE
        }
    }
}
