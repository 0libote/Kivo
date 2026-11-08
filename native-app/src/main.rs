mod ai;
mod app;
mod flow_writing;
mod keystore;
mod platform;
mod settings;
mod settings_onboarding;
mod surfaces;
mod theme;

fn main() -> eframe::Result<()> {
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 740.0])
            .with_min_inner_size([820.0, 560.0])
            .with_title("Kivo"),
        ..Default::default()
    };
    // Software-GL environments (Xvfb, old drivers) can't always bring up the
    // default wgpu backend; allow forcing the OpenGL renderer without a rebuild.
    if std::env::var("KIVO_RENDERER").as_deref() == Ok("glow") {
        options.renderer = eframe::Renderer::Glow;
    }
    eframe::run_native(
        "Kivo",
        options,
        Box::new(|cc| Ok(Box::new(app::KivoApp::new(cc)))),
    )
}
