mod ai;
mod app;
mod flow_writing;
mod platform;
mod settings;
mod settings_onboarding;
mod surfaces;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([860.0, 620.0])
            .with_title("Kivo"),
        ..Default::default()
    };
    eframe::run_native(
        "Kivo",
        options,
        Box::new(|cc| Ok(Box::new(app::KivoApp::new(cc)))),
    )
}
