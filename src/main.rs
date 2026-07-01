//! Pulse — a live hardware status monitor for the macOS menu bar.
//!
//! A `tray-icon` status item drives a hidden, borderless `egui`/`eframe`
//! popover. All sampling happens on a background thread (`sysinfo` +
//! `macmon`), so the UI never blocks.

// No console window on release; harmless on macOS but keeps parity if built elsewhere.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod autostart;
mod config;
mod metrics;
mod theme;
mod tray;
mod util;
mod widgets;

use config::Config;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn main() -> eframe::Result<()> {
    let cfg = Config::load();
    let poll_ms = Arc::new(AtomicU64::new(cfg.interval_ms()));

    // Channel: sampler thread -> UI thread.
    let (sample_tx, sample_rx) = crossbeam_channel::unbounded();

    let viewport = egui::ViewportBuilder::default()
        .with_title("Pulse")
        .with_inner_size([app::WINDOW_W, app::WINDOW_H])
        .with_min_inner_size([app::WINDOW_W, 120.0])
        .with_max_inner_size([app::WINDOW_W, app::WINDOW_H])
        // Start off-screen and hidden so nothing ever flashes on launch.
        .with_position([-10_000.0, -10_000.0])
        .with_decorations(false)
        .with_titlebar_shown(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_taskbar(false)
        .with_has_shadow(false)
        .with_active(false)
        .with_visible(false)
        .with_always_on_top();

    let native_options = eframe::NativeOptions {
        viewport,
        // Run as a macOS "accessory": no Dock icon, no Cmd-Tab entry, even when
        // launched unbundled during development.
        event_loop_builder: Some(Box::new(|builder| {
            use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
            builder.with_activation_policy(ActivationPolicy::Accessory);
        })),
        run_and_return: false,
        ..Default::default()
    };

    let poll_for_sampler = poll_ms.clone();
    eframe::run_native(
        "Pulse",
        native_options,
        Box::new(move |cc| {
            // Sampler needs the egui context to request repaints.
            metrics::spawn(cc.egui_ctx.clone(), sample_tx, poll_for_sampler);
            Ok(Box::new(app::PulseApp::new(cc, sample_rx, poll_ms, cfg)))
        }),
    )
}
