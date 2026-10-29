#![allow(non_snake_case)]

mod app;
mod app_paths;
mod arduino_build;
mod arduino_defines;
mod arduino_vpp_config;
mod atomic_file;
mod bounded_process;
mod codesys_import;
mod compiler_adapter;
mod crash_log;
mod debug_values;
mod debug_wire;
mod generated_project;
mod icons;
mod library_manager;
mod local_runtime;
mod native_clipboard;
mod new_project;
mod plc_model;
mod pou_parser;
mod pou_serializer;
mod project;
mod protocol_config;
mod public_library_catalog;
mod remote_debug;
mod runtime_debug_ws;
mod runtime_v4_bundle;
mod runtime_v4_client;
mod save_modals;
mod style_contract;
mod variable_parser;
mod vendor_screen;
mod vpp_package;
#[cfg(test)]
mod workflow_parity;
mod workspace;
mod workspace_icons;

use dioxus_desktop::{Config, LogicalSize, WindowBuilder};

fn main() {
    crash_log::install();
    let config = Config::new()
        .with_disable_context_menu(true)
        // Dioxus 0.7.x initially hides macOS windows until the WebView sends its
        // initialization callback. On macOS 26 that callback can be delayed or
        // skipped in optimized bundles; reveal the native window before WebView
        // creation so the application cannot become a healthy-but-windowless process.
        .with_on_window(|window, _| window.set_visible(true))
        .with_window(
            WindowBuilder::new()
                .with_title("RustPLC Editor")
                .with_inner_size(LogicalSize::new(1440.0, 768.0))
                .with_min_inner_size(LogicalSize::new(1124.0, 628.0))
                .with_maximized(true)
                .with_decorations(false),
        );
    dioxus_desktop::launch::launch(app::App, Vec::new(), vec![Box::new(config)]);
}
