mod app;

use app::SwitchboardApp;
use eframe::NativeOptions;

fn main() -> eframe::Result<()> {
    let native_options = NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Switchboard",
        native_options,
        Box::new(|cc| Ok(Box::new(SwitchboardApp::new(cc)))),
    )
}
