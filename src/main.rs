#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod instance;
#[cfg(test)]
mod integration_tests;
mod model;
mod net;
mod rdp;
mod sftp;
mod ssh_config;
mod storage;
mod terminal;
mod tunnels;
mod ui;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let window_title = if args.iter().any(|arg| arg == "--preview") {
        "Nexus Remote — Aperçu"
    } else {
        "Nexus Remote"
    };
    let data_dir = args
        .iter()
        .position(|s| s == "--data-dir")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(storage::Store::default_dir);
    let _instance = match instance::InstanceGuard::acquire(&data_dir) {
        Ok(guard) => guard,
        Err(error) => {
            rfd::MessageDialog::new()
                .set_title("Nexus Remote")
                .set_description(error)
                .set_level(rfd::MessageLevel::Info)
                .show();
            return Ok(());
        }
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(window_title)
            .with_decorations(false)
            .with_resizable(true)
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/nexus.png"))
                    .expect("Icône intégrée"),
            )
            .with_inner_size([1320.0, 850.0])
            .with_min_inner_size([1040.0, 700.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "Nexus Remote",
        options,
        Box::new(|cc| Ok(Box::new(app::NexusApp::new(cc)))),
    );
    if let Err(error) = &result {
        rfd::MessageDialog::new()
            .set_title("Nexus Remote — démarrage impossible")
            .set_description(format!(
                "{error}\n\nVérifiez que le pilote graphique prend en charge OpenGL 3.3."
            ))
            .set_level(rfd::MessageLevel::Error)
            .show();
    }
    result
}
