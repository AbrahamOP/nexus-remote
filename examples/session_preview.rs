//! Local visual QA only. Built with `cargo rustc --example session_preview -- --cfg test`.
//! Reuses the actual UI with offline fixtures; no catalog, credentials or sockets.
#![allow(dead_code, unused_imports)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[path = "../src/app.rs"]
mod app;
#[path = "../src/instance.rs"]
mod instance;
#[cfg(test)]
#[path = "../src/integration_tests.rs"]
mod integration_tests;
#[path = "../src/model.rs"]
mod model;
#[path = "../src/net.rs"]
mod net;
#[path = "../src/rdp.rs"]
mod rdp;
#[path = "../src/sftp.rs"]
mod sftp;
#[path = "../src/ssh_config.rs"]
mod ssh_config;
#[path = "../src/storage.rs"]
mod storage;
#[path = "../src/terminal.rs"]
mod terminal;
#[path = "../src/tunnels.rs"]
mod tunnels;
#[path = "../src/ui.rs"]
mod ui;

#[cfg(test)]
fn main() -> eframe::Result {
    let args: Vec<_> = std::env::args().collect();
    let rdp = args.iter().any(|arg| arg == "--rdp");
    let split = args.iter().any(|arg| arg == "--split");
    eframe::run_native(
        "Nexus sessions visual QA",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("Nexus Remote — Aperçu hors ligne")
                .with_inner_size([1320.0, 850.0])
                .with_min_inner_size([1040.0, 700.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            Ok(Box::new(app::NexusApp::offline_session_preview(
                &cc.egui_ctx,
                rdp,
                split,
            )))
        }),
    )
}

#[cfg(not(test))]
fn main() {
    eprintln!(
        "Build this offline QA example with cargo rustc --example session_preview -- --cfg test"
    );
}
