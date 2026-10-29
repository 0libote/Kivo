mod ai;
mod app;
mod flow_writing;
mod platform;
mod settings;
mod settings_onboarding;
mod surfaces;
mod theme;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([920.0, 680.0])
            .with_min_inner_size([620.0, 460.0])
            .with_title("Kivo"),
        ..Default::default()
    };
    eframe::run_native(
        "Kivo",
        options,
        Box::new(|cc| Ok(Box::new(app::KivoApp::new(cc)))),
    )
}
