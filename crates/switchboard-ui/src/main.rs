mod app;

use app::SwitchboardApp;
use eframe::NativeOptions;

fn main() -> eframe::Result<()> {
    let native_options = NativeOptions::default();
    eframe::run_native(
        "Switchboard",
        native_options,
        Box::new(|cc| Ok(Box::new(SwitchboardApp::new(cc)))),
    )
}
