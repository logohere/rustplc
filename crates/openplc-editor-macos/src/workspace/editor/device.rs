use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use dioxus::prelude::*;
use serde_json::{json, Map, Value};

use crate::app::{AppStore, ConsoleEntry};
use crate::arduino_build;
use crate::project::ProjectService;
use crate::vpp_package::{apply_platform_options, InstalledVppBoard, VppPackageManager};

use super::remote_runtime::RemoteRuntimeSlot;

const BUILTIN_BOARDS: &[&str] = &[
    "OpenPLC Simulator",
    "OpenPLC Runtime v3",
    "OpenPLC Runtime v4",
];

const PIN_TYPES: &[(&str, &str)] = &[
    ("digitalInput", "Digital Input"),
    ("digitalOutput", "Digital Output"),
    ("analogInput", "Analog Input"),
    ("analogOutput", "Analog Output"),
];

#[component]
pub fn DeviceConfigurationEditor() -> Element {
    let mut store = use_context::<AppStore>();
    let project_service = use_context::<ProjectService>();
    let mut selected_pin = use_signal(|| None::<usize>);
    let mut error = use_signal(|| None::<String>);
    let mut package_revision = use_signal(|| 0u64);
    let _ = package_revision();
    let package_manager = VppPackageManager::new().ok();
    let installed_packages = package_manager
        .as_ref()
        .map(VppPackageManager::list_installed)
        .unwrap_or_default();
    let installed_boards = package_manager
        .as_ref()
        .map(VppPackageManager::installed_boards)
        .unwrap_or_default();
    let content = store.current_content();
    let Ok(buffer) = serde_json::from_str::<Value>(&content) else {
        return rsx! {
            div { class: "flex h-full w-full items-center justify-center rounded-lg border border-red-200 bg-red-50 p-4 font-caption text-xs text-red-600 dark:border-red-900 dark:bg-red-950/20 dark:text-red-400",
                "Invalid device configuration JSON."
            }
        };
    };
    let configuration = buffer
        .get("configuration")
        .cloned()
        .filter(Value::is_object)
        .unwrap_or_else(default_device_configuration);
    let board = configuration
        .get("deviceBoard")
        .and_then(Value::as_str)
        .unwrap_or("OpenPLC Simulator")
        .to_owned();
    let communication_port = configuration
        .get("communicationPort")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let runtime_ip = configuration
        .get("runtimeIpAddress")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let allow_insecure_runtime_tls = configuration
        .get("allowInsecureRuntimeTls")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let boards = available_board_names(&buffer, &board, &installed_boards);
    let vpp_board = installed_boards
        .iter()
        .find(|candidate| candidate.device.name == board)
        .cloned();
    let pins = board_pins(&buffer, &board);
    let platform_options = configuration
        .get("selectedPlatformOptions")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let selected_platform_options = platform_options
        .iter()
        .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_owned())))
        .collect::<BTreeMap<_, _>>();
    let effective_platform = vpp_board.as_ref().and_then(|candidate| {
        candidate.device.target.platform.as_ref().map(|platform| {
            apply_platform_options(
                platform,
                &candidate.device.target.platform_options,
                &selected_platform_options,
            )
        })
    });
    let show_pin_mapping = pin_mapping_enabled_for_known_state(&board, &pins, vpp_board.as_ref());
    let runtime_target = is_runtime_board(&board)
        || vpp_board
            .as_ref()
            .is_some_and(|candidate| candidate.device.target.target_type == "runtime-v4");
    let arduino_target = vpp_board
        .as_ref()
        .is_some_and(|candidate| candidate.device.target.target_type == "arduino-cli");
    let board_for_add = board.clone();
    let board_for_remove = board.clone();

    rsx! {
        div { class: "flex h-full w-full flex-col gap-6 overflow-y-auto pr-1",
            div { class: "flex flex-wrap items-start gap-8 rounded-lg border border-neutral-200 p-4 dark:border-neutral-800",
                div { class: "flex w-[380px] flex-col gap-3",
                    h2 { class: "font-caption text-lg font-medium text-neutral-950 dark:text-white", "Board Settings" }
                    label { class: "flex items-center gap-3 font-caption text-xs font-medium text-neutral-950 dark:text-white",
                        span { class: "w-28", "Device" }
                        select {
                            class: "h-8 min-w-0 flex-1 rounded-md border border-neutral-200 bg-white px-2 text-xs outline-none focus:border-brand dark:border-neutral-800 dark:bg-neutral-950",
                            value: "{board}",
                            onchange: {
                                let installed_boards = installed_boards.clone();
                                move |event| {
                                    let next = event.value();
                                    let vpp = installed_boards.iter().find(|candidate| candidate.device.name == next);
                                    mutate_device(&mut store, |buffer| {
                                        switch_device_board(buffer, &next);
                                        if let Some(vpp) = vpp {
                                            apply_vpp_defaults(buffer, &next, vpp);
                                        }
                                    });
                                    selected_pin.set(None);
                                    error.set(None);
                                }
                            },
                            for choice in boards.iter() {
                                {
                                    let label = installed_boards
                                        .iter()
                                        .find(|candidate| candidate.device.name == *choice)
                                        .map(|candidate| format!("{} — {}", candidate.vendor, choice))
                                        .unwrap_or_else(|| choice.clone());
                                    rsx! { option { key: "board-{choice}", value: "{choice}", "{label}" } }
                                }
                            }
                        }
                    }
                    if board == "OpenPLC Simulator" {
                        p { class: "font-caption text-xs leading-5 text-neutral-500",
                            "Built-in simulator — no board configuration is required. Build runs through the in-process simulator target."
                        }
                    } else if runtime_target {
                        label { class: "flex items-center gap-3 font-caption text-xs font-medium text-neutral-950 dark:text-white",
                            span { class: "w-28", "IP Address" }
                            input {
                                class: "h-8 min-w-0 flex-1 rounded-md border border-neutral-200 bg-transparent px-2 font-mono text-xs outline-none focus:border-brand dark:border-neutral-800",
                                value: "{runtime_ip}",
                                placeholder: "127.0.0.1 or localhost",
                                oninput: move |event| set_configuration_string(&mut store, "runtimeIpAddress", event.value()),
                            }
                        }
                        label { class: "flex items-center gap-2 font-caption text-[10px] text-neutral-600 dark:text-neutral-400",
                            input {
                                r#type: "checkbox",
                                checked: allow_insecure_runtime_tls,
                                onchange: move |event| set_configuration_bool(&mut store, "allowInsecureRuntimeTls", event.checked()),
                            }
                            "Allow unverified/self-signed Runtime TLS"
                        }
                        if allow_insecure_runtime_tls {
                            p { role: "alert", class: "rounded border border-amber-200 bg-amber-50 px-2 py-1 font-caption text-[10px] text-amber-700 dark:border-amber-900 dark:bg-amber-950/20 dark:text-amber-300",
                                "TLS certificate and hostname verification are disabled for this Runtime. Credentials may be exposed to a man-in-the-middle attacker."
                            }
                        } else {
                            p { class: "font-caption text-[10px] leading-4 text-neutral-500",
                                "Runtime HTTPS and debug WebSocket certificates must validate normally."
                            }
                        }
                        RemoteRuntimeSlot {
                            ip: runtime_ip.clone(),
                            board: board.clone(),
                            allow_insecure_tls: allow_insecure_runtime_tls,
                        }
                    } else {
                        label { class: "flex items-center gap-3 font-caption text-xs font-medium text-neutral-950 dark:text-white",
                            span { class: "w-28", "Communication Port" }
                            input {
                                class: "h-8 min-w-0 flex-1 rounded-md border border-neutral-200 bg-transparent px-2 font-mono text-xs outline-none focus:border-brand dark:border-neutral-800",
                                value: "{communication_port}",
                                placeholder: "/dev/cu.usbmodem...",
                                oninput: move |event| set_configuration_string(&mut store, "communicationPort", event.value()),
                            }
                        }
                        p { class: "font-caption text-[10px] leading-4 text-neutral-500",
                            if let Some(vpp) = vpp_board.as_ref() {
                                "Installed VPP target · {vpp.vendor} · package {vpp.package_id}"
                            } else {
                                "External target preserved from project state."
                            }
                        }
                        if let Some(platform) = effective_platform.as_ref() {
                            p { class: "font-mono text-[9px] leading-4 text-neutral-500", "FQBN: {platform}" }
                        }
                        if arduino_target {
                            div { class: "mt-2 flex flex-wrap gap-2",
                                button {
                                    r#type: "button",
                                    disabled: (store.is_compiling)(),
                                    class: "h-8 rounded-md border border-neutral-200 bg-white px-3 font-caption text-[10px] font-medium text-neutral-700 hover:border-brand hover:text-brand disabled:opacity-40 dark:border-neutral-800 dark:bg-neutral-950 dark:text-neutral-200",
                                    onclick: {
                                        let service = project_service.clone();
                                        let board_name = board.clone();
                                        let project_signal = store.project;
                                        let mut console = store.console;
                                        let mut compiling = store.is_compiling;
                                        move |_| {
                                            if let Err(message) = store.save_all(&service) {
                                                error.set(Some(message));
                                                return;
                                            }
                                            let Some(project) = project_signal() else { return; };
                                            let board_name = board_name.clone();
                                            compiling.set(true);
                                            spawn(async move {
                                                let result = tokio::task::spawn_blocking(move || {
                                                    arduino_build::build_project(&project, &board_name, false)
                                                }).await;
                                                match result {
                                                    Ok(Ok(report)) => {
                                                        console.with_mut(|entries| {
                                                            if !report.diagnostics.trim().is_empty() {
                                                                entries.push(ConsoleEntry { level: "info", message: report.diagnostics });
                                                            }
                                                            entries.push(ConsoleEntry {
                                                                level: "info",
                                                                message: format!("Arduino build successful: {} · {} → {}", report.board, report.fqbn, report.build_dir.display()),
                                                            });
                                                        });
                                                        error.set(None);
                                                    }
                                                    Ok(Err(message)) => error.set(Some(message)),
                                                    Err(join) => error.set(Some(format!("Arduino build task failed: {join}"))),
                                                }
                                                compiling.set(false);
                                            });
                                        }
                                    },
                                    "Build Firmware"
                                }
                                button {
                                    r#type: "button",
                                    disabled: (store.is_compiling)() || communication_port.trim().is_empty(),
                                    class: "h-8 rounded-md bg-brand px-3 font-caption text-[10px] font-medium text-white hover:bg-brand-medium-dark disabled:opacity-40",
                                    onclick: {
                                        let service = project_service.clone();
                                        let board_name = board.clone();
                                        let project_signal = store.project;
                                        let mut console = store.console;
                                        let mut compiling = store.is_compiling;
                                        move |_| {
                                            if let Err(message) = store.save_all(&service) {
                                                error.set(Some(message));
                                                return;
                                            }
                                            let Some(project) = project_signal() else { return; };
                                            let board_name = board_name.clone();
                                            compiling.set(true);
                                            spawn(async move {
                                                let result = tokio::task::spawn_blocking(move || {
                                                    arduino_build::build_project(&project, &board_name, true)
                                                }).await;
                                                match result {
                                                    Ok(Ok(report)) => {
                                                        console.with_mut(|entries| {
                                                            if !report.diagnostics.trim().is_empty() {
                                                                entries.push(ConsoleEntry { level: "info", message: report.diagnostics });
                                                            }
                                                            entries.push(ConsoleEntry {
                                                                level: "info",
                                                                message: format!("Arduino upload successful: {} · {}", report.board, report.fqbn),
                                                            });
                                                        });
                                                        error.set(None);
                                                    }
                                                    Ok(Err(message)) => error.set(Some(message)),
                                                    Err(join) => error.set(Some(format!("Arduino upload task failed: {join}"))),
                                                }
                                                compiling.set(false);
                                            });
                                        }
                                    },
                                    "Build & Upload"
                                }
                            }
                        }
                    }
                    if vpp_board.as_ref().is_some_and(|board| !board.device.target.platform_options.is_empty()) {
                        div { class: "mt-2 flex flex-col gap-2",
                            h3 { class: "font-caption text-xs font-semibold text-neutral-800 dark:text-neutral-200", "Platform Options" }
                            for option in vpp_board.as_ref().map(|board| board.device.target.platform_options.as_slice()).into_iter().flatten() {
                                {
                                    let option_key = option.key.clone();
                                    let current = platform_options
                                        .get(&option.key)
                                        .and_then(Value::as_str)
                                        .unwrap_or(&option.default)
                                        .to_owned();
                                    rsx! {
                                        label { key: "platform-{option_key}", class: "flex items-center gap-2 font-caption text-[10px]",
                                            span { class: "w-28 truncate text-neutral-500", "{option.label}" }
                                            select {
                                                class: "h-7 min-w-0 flex-1 rounded border border-neutral-200 bg-transparent px-2 font-mono text-[10px] outline-none focus:border-brand dark:border-neutral-800",
                                                value: "{current}",
                                                onchange: move |event| set_platform_option(&mut store, &option_key, event.value()),
                                                for value in option.values.iter() {
                                                    option { key: "{option_key}-{value.id}", value: "{value.id}", "{value.label}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else if !platform_options.is_empty() {
                        div { class: "mt-2 flex flex-col gap-2",
                            h3 { class: "font-caption text-xs font-semibold text-neutral-800 dark:text-neutral-200", "Selected Platform Options" }
                            for (key, value) in platform_options.iter() {
                                {
                                    let option_key = key.clone();
                                    let option_value = value.as_str().unwrap_or_default().to_owned();
                                    rsx! {
                                        label { key: "platform-{option_key}", class: "flex items-center gap-2 font-caption text-[10px]",
                                            span { class: "w-28 truncate text-neutral-500", "{option_key}" }
                                            input {
                                                class: "h-7 min-w-0 flex-1 rounded border border-neutral-200 bg-transparent px-2 font-mono text-[10px] outline-none focus:border-brand dark:border-neutral-800",
                                                value: "{option_value}",
                                                oninput: move |event| set_platform_option(&mut store, &option_key, event.value()),
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "flex min-h-52 min-w-64 flex-1 items-center justify-center rounded-lg bg-neutral-50 p-6 dark:bg-neutral-900",
                    div { class: "flex flex-col items-center gap-3 text-center",
                        div { class: "flex h-20 w-28 items-center justify-center rounded-xl border-2 border-neutral-300 bg-white font-mono text-xs font-semibold text-neutral-500 shadow-sm dark:border-neutral-700 dark:bg-neutral-950",
                            if board == "OpenPLC Simulator" { "SIM" } else if board.contains("Runtime") { "RUNTIME" } else { "TARGET" }
                        }
                        span { class: "max-w-72 font-caption text-xs font-medium text-neutral-700 dark:text-neutral-300", "{board}" }
                    }
                }
            }

            div { class: "flex flex-col gap-2 rounded-lg border border-neutral-200 p-4 dark:border-neutral-800",
                div { class: "flex items-center justify-between gap-3",
                    div {
                        h2 { class: "font-caption text-sm font-semibold text-neutral-950 dark:text-white", "VPP Packages" }
                        p { class: "font-caption text-[10px] text-neutral-500", "Signed Vendor Plugin Packages are verified before installation." }
                    }
                    button {
                        r#type: "button",
                        class: "h-8 rounded-md border border-neutral-200 bg-white px-3 font-caption text-[10px] font-medium text-neutral-700 hover:border-brand hover:text-brand dark:border-neutral-800 dark:bg-neutral-950 dark:text-neutral-200",
                        onclick: move |_| {
                            let Some(path) = rfd::FileDialog::new()
                                .set_title("Import OpenPLC Vendor Plugin Package")
                                .add_filter("Vendor Plugin Package", &["vpp"])
                                .pick_file()
                            else { return; };
                            let result = VppPackageManager::new().and_then(|manager| manager.import_from_file(&path));
                            match result {
                                Ok(installed) => {
                                    store.console.with_mut(|console| console.push(ConsoleEntry {
                                        level: "info",
                                        message: format!("Installed VPP package {}@{}", installed.package_id, installed.version),
                                    }));
                                    package_revision += 1;
                                    error.set(None);
                                }
                                Err(message) => error.set(Some(message)),
                            }
                        },
                        "Import .vpp"
                    }
                }
                if installed_packages.is_empty() {
                    p { class: "font-caption text-[10px] text-neutral-500", "No VPP packages installed." }
                } else {
                    for package in installed_packages.iter() {
                        {
                            let package_id = package.package_id.clone();
                            let active_package = vpp_board.as_ref().is_some_and(|board| board.package_id == package_id);
                            rsx! {
                                div { key: "vpp-{package_id}", class: "flex items-center gap-2 rounded border border-neutral-100 px-2 py-1 dark:border-neutral-900",
                                    span { class: "min-w-0 flex-1 truncate font-mono text-[10px]", "{package.package_id} · {package.version}" }
                                    button {
                                        r#type: "button",
                                        disabled: active_package,
                                        title: if active_package { "Switch away from this package's board before uninstalling" } else { "Uninstall package" },
                                        class: "rounded px-2 py-1 font-caption text-[9px] text-neutral-500 hover:text-red-600 disabled:opacity-30",
                                        onclick: move |_| {
                                            match VppPackageManager::new().and_then(|manager| manager.uninstall(&package_id)) {
                                                Ok(()) => { package_revision += 1; error.set(None); },
                                                Err(message) => error.set(Some(message)),
                                            }
                                        },
                                        "Remove"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if show_pin_mapping {
                div { class: "flex flex-col gap-2",
                    div { class: "flex h-8 items-center justify-between",
                        div {
                            h2 { class: "font-caption text-lg font-medium text-neutral-950 dark:text-white", "Pin Mapping" }
                            p { class: "font-caption text-[10px] text-neutral-500", "Mappings are stored independently for each target board." }
                        }
                        div { class: "flex items-center gap-1",
                            button {
                                r#type: "button",
                                class: "flex h-7 w-7 items-center justify-center rounded-md text-sm font-semibold text-brand hover:bg-neutral-100 dark:hover:bg-neutral-900",
                                title: "Add pin mapping row",
                                onclick: move |_| {
                                    let next = add_pin_for_board(&mut store, &board_for_add, selected_pin());
                                    selected_pin.set(next);
                                    error.set(None);
                                },
                                "+"
                            }
                            button {
                                r#type: "button",
                                class: "flex h-7 w-7 items-center justify-center rounded-md text-sm font-semibold text-brand hover:bg-neutral-100 disabled:opacity-30 dark:hover:bg-neutral-900",
                                title: "Remove selected pin mapping row",
                                disabled: selected_pin().is_none(),
                                onclick: move |_| {
                                    let next = remove_pin_for_board(&mut store, &board_for_remove, selected_pin());
                                    selected_pin.set(next);
                                    error.set(None);
                                },
                                "−"
                            }
                        }
                    }
                    if let Some(message) = error() {
                        div { class: "rounded border border-red-200 bg-red-50 px-2 py-1 font-caption text-[10px] text-red-600 dark:border-red-900 dark:bg-red-950/20 dark:text-red-400", "{message}" }
                    }
                    div { class: "overflow-x-auto rounded-lg border border-neutral-200 dark:border-neutral-800",
                        table { class: "w-full min-w-[760px] table-fixed border-collapse font-caption text-xs",
                            thead { class: "bg-neutral-50 dark:bg-neutral-900",
                                tr {
                                    th { class: "w-[24%] px-2 py-2 text-left font-medium", "Pin" }
                                    th { class: "w-[24%] px-2 py-2 text-left font-medium", "Type" }
                                    th { class: "w-[24%] px-2 py-2 text-left font-medium", "Address" }
                                    th { class: "w-[28%] px-2 py-2 text-left font-medium", "Alias" }
                                }
                            }
                            tbody {
                                for (index, pin) in pins.iter().enumerate() {
                                    {
                                        let pin_name = pin.get("pin").and_then(Value::as_str).unwrap_or_default().to_owned();
                                        let pin_type = pin.get("pinType").and_then(Value::as_str).unwrap_or("digitalInput").to_owned();
                                        let address = pin.get("address").and_then(Value::as_str).unwrap_or_default().to_owned();
                                        let alias = pin.get("alias").and_then(Value::as_str).unwrap_or_default().to_owned();
                                        let board_for_name = board.clone();
                                        let board_for_type = board.clone();
                                        let board_for_alias = board.clone();
                                        let board_for_lookup = board.clone();
                                        let pin_name_for_lookup = pin_name.clone();
                                        rsx! {
                                            tr {
                                                key: "pin-{board}-{index}",
                                                class: if selected_pin() == Some(index) { "bg-brand/5" } else { "border-t border-neutral-100 dark:border-neutral-900" },
                                                onclick: move |_| selected_pin.set(Some(index)),
                                                td { class: "p-1",
                                                    input {
                                                        class: "h-8 w-full bg-transparent px-2 font-mono text-xs outline-none",
                                                        value: "{pin_name}",
                                                        placeholder: "D13",
                                                        oninput: move |event| {
                                                            match update_pin_name(&mut store, &board_for_name, index, &event.value()) {
                                                                Ok(()) => error.set(None),
                                                                Err(message) => error.set(Some(message)),
                                                            }
                                                        }
                                                    }
                                                }
                                                td { class: "p-1",
                                                    select {
                                                        class: "h-8 w-full bg-transparent px-1 text-xs outline-none",
                                                        value: "{pin_type}",
                                                        onchange: move |event| {
                                                            if let Err(message) = change_pin_type(&mut store, &board_for_type, index, &event.value()) {
                                                                error.set(Some(message));
                                                            } else {
                                                                selected_pin.set(find_pin_index_by_name(&store, &board_for_lookup, &pin_name_for_lookup));
                                                                error.set(None);
                                                            }
                                                        },
                                                        for (value, label) in PIN_TYPES {
                                                            option { key: "pin-type-{value}", value: "{value}", "{label}" }
                                                        }
                                                    }
                                                }
                                                td { class: "p-1",
                                                    div { class: "h-8 w-full px-2 py-2 font-mono text-xs text-neutral-500", "{address}" }
                                                }
                                                td { class: "p-1",
                                                    input {
                                                        class: "h-8 w-full bg-transparent px-2 font-mono text-xs outline-none",
                                                        value: "{alias}",
                                                        placeholder: "sensor_alias",
                                                        oninput: move |event| {
                                                            match update_pin_alias(&mut store, &board_for_alias, index, &event.value()) {
                                                                Ok(()) => error.set(None),
                                                                Err(message) => error.set(Some(message)),
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if pins.is_empty() {
                                    tr { td { colspan: "4", class: "px-3 py-8 text-center text-neutral-500", "No pin mappings for this board." } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn mutate_device(store: &mut AppStore, mutate: impl FnOnce(&mut Value)) {
    let content = store.current_content();
    let Ok(mut buffer) = serde_json::from_str::<Value>(&content) else {
        return;
    };
    mutate(&mut buffer);
    if let Ok(content) = serde_json::to_string_pretty(&buffer) {
        store.set_current_content(content);
    }
}

fn default_device_configuration() -> Value {
    json!({
        "deviceBoard": "OpenPLC Simulator",
        "communicationPort": "",
        "runtimeIpAddress": ""
    })
}

fn available_board_names(
    buffer: &Value,
    active: &str,
    vpp_boards: &[InstalledVppBoard],
) -> Vec<String> {
    let mut names = BTreeSet::new();
    for name in BUILTIN_BOARDS {
        names.insert((*name).to_owned());
    }
    if !active.is_empty() {
        names.insert(active.to_owned());
    }
    names.extend(vpp_boards.iter().map(|board| board.device.name.clone()));
    if let Some(mapping) = buffer.get("pinMapping").and_then(Value::as_object) {
        names.extend(mapping.keys().cloned());
    }
    if let Some(by_board) = buffer
        .get("configuration")
        .and_then(|configuration| configuration.get("vendorScreenDataByBoard"))
        .and_then(Value::as_object)
    {
        names.extend(by_board.keys().cloned());
    }
    let mut names = names.into_iter().collect::<Vec<_>>();
    names.sort_by(|left, right| board_order(left, right));
    names
}

fn board_order(left: &str, right: &str) -> Ordering {
    let rank = |value: &str| {
        BUILTIN_BOARDS
            .iter()
            .position(|candidate| candidate == &value)
            .unwrap_or(BUILTIN_BOARDS.len())
    };
    rank(left)
        .cmp(&rank(right))
        .then_with(|| left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase()))
}

fn is_runtime_board(board: &str) -> bool {
    board == "OpenPLC Runtime v3" || board == "OpenPLC Runtime v4"
}

fn pin_mapping_enabled_for_known_state(
    board: &str,
    pins: &[Value],
    vpp: Option<&InstalledVppBoard>,
) -> bool {
    let runtime_vpp = vpp.is_some_and(|board| board.device.target.target_type == "runtime-v4");
    let capability = vpp
        .and_then(|board| board.device.capabilities.get("pinMapping"))
        .and_then(Value::as_bool);
    if board == "OpenPLC Simulator" || is_runtime_board(board) || runtime_vpp {
        return capability.unwrap_or(!pins.is_empty());
    }
    if let Some(enabled) = capability {
        return enabled;
    }
    true
}

fn apply_vpp_defaults(buffer: &mut Value, board_name: &str, board: &InstalledVppBoard) {
    let Some(configuration) = buffer
        .get_mut("configuration")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let options = board
        .device
        .target
        .platform_options
        .iter()
        .map(|option| (option.key.clone(), Value::String(option.default.clone())))
        .collect::<Map<_, _>>();
    configuration.insert("selectedPlatformOptions".to_owned(), Value::Object(options));
    if let Some(ip) = board.device.defaults.runtime_ip_address.as_ref() {
        configuration.insert("runtimeIpAddress".to_owned(), Value::String(ip.clone()));
    }
    if let Some(root) = buffer.as_object_mut() {
        let mapping = root
            .entry("pinMapping".to_owned())
            .or_insert_with(|| json!({}));
        if let Some(mapping) = mapping.as_object_mut() {
            let existing = mapping
                .entry(board_name.to_owned())
                .or_insert_with(|| Value::Array(Vec::new()));
            if existing.as_array().is_some_and(Vec::is_empty) {
                let mut pins = Vec::new();
                for (pin_type, names) in [
                    ("digitalInput", &board.device.defaults.pins.default_din),
                    ("digitalOutput", &board.device.defaults.pins.default_dout),
                    ("analogInput", &board.device.defaults.pins.default_ain),
                    ("analogOutput", &board.device.defaults.pins.default_aout),
                ] {
                    for name in names {
                        let address =
                            increment_address(&highest_address(&pins, pin_type), pin_type);
                        let mut value = pin_value(pin_type, address);
                        if let Some(object) = value.as_object_mut() {
                            object.insert("pin".to_owned(), Value::String(name.clone()));
                        }
                        pins.push(value);
                    }
                }
                *existing = Value::Array(pins);
            }
        }
    }
}

fn switch_device_board(buffer: &mut Value, next_board: &str) {
    if next_board.trim().is_empty() {
        return;
    }
    let Some(root) = buffer.as_object_mut() else {
        return;
    };
    let configuration = root
        .entry("configuration".to_owned())
        .or_insert_with(default_device_configuration);
    let Some(configuration) = configuration.as_object_mut() else {
        return;
    };
    let previous_board = configuration
        .get("deviceBoard")
        .and_then(Value::as_str)
        .unwrap_or("OpenPLC Simulator")
        .to_owned();
    if previous_board == next_board {
        return;
    }

    let active_vendor_data = configuration
        .get("vendorScreenData")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let archive = configuration
        .entry("vendorScreenDataByBoard".to_owned())
        .or_insert_with(|| json!({}));
    if let Some(archive) = archive.as_object_mut() {
        archive.insert(previous_board.clone(), active_vendor_data);
        let next_vendor = archive
            .get(next_board)
            .cloned()
            .unwrap_or_else(|| json!({}));
        configuration.insert("vendorScreenData".to_owned(), next_vendor);
    }
    configuration.insert(
        "selectedPlatformOptions".to_owned(),
        Value::Object(Map::new()),
    );
    configuration.insert(
        "deviceBoard".to_owned(),
        Value::String(next_board.to_owned()),
    );

    let mapping = root
        .entry("pinMapping".to_owned())
        .or_insert_with(|| json!({}));
    if let Some(mapping) = mapping.as_object_mut() {
        mapping
            .entry(next_board.to_owned())
            .or_insert_with(|| Value::Array(Vec::new()));
    }
}

fn set_configuration_string(store: &mut AppStore, key: &str, value: String) {
    mutate_device(store, |buffer| {
        if let Some(configuration) = buffer
            .get_mut("configuration")
            .and_then(Value::as_object_mut)
        {
            configuration.insert(key.to_owned(), Value::String(value));
        }
    });
}

fn set_configuration_bool(store: &mut AppStore, key: &str, value: bool) {
    mutate_device(store, |buffer| {
        if let Some(configuration) = buffer
            .get_mut("configuration")
            .and_then(Value::as_object_mut)
        {
            configuration.insert(key.to_owned(), Value::Bool(value));
        }
    });
}

fn set_platform_option(store: &mut AppStore, key: &str, value: String) {
    mutate_device(store, |buffer| {
        let Some(configuration) = buffer
            .get_mut("configuration")
            .and_then(Value::as_object_mut)
        else {
            return;
        };
        let options = configuration
            .entry("selectedPlatformOptions".to_owned())
            .or_insert_with(|| json!({}));
        if let Some(options) = options.as_object_mut() {
            options.insert(key.to_owned(), Value::String(value));
        }
    });
}

fn board_pins(buffer: &Value, board: &str) -> Vec<Value> {
    buffer
        .get("pinMapping")
        .and_then(Value::as_object)
        .and_then(|mapping| mapping.get(board))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn with_board_pins_mut(buffer: &mut Value, board: &str, mutate: impl FnOnce(&mut Vec<Value>)) {
    let Some(root) = buffer.as_object_mut() else {
        return;
    };
    let mapping = root
        .entry("pinMapping".to_owned())
        .or_insert_with(|| json!({}));
    let Some(mapping) = mapping.as_object_mut() else {
        return;
    };
    let bucket = mapping
        .entry(board.to_owned())
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Some(bucket) = bucket.as_array_mut() {
        mutate(bucket);
    }
}

fn add_pin_for_board(store: &mut AppStore, board: &str, selected: Option<usize>) -> Option<usize> {
    let mut next_selected = None;
    mutate_device(store, |buffer| {
        with_board_pins_mut(buffer, board, |pins| {
            next_selected = Some(add_pin(pins, selected));
        });
    });
    next_selected
}

fn remove_pin_for_board(
    store: &mut AppStore,
    board: &str,
    selected: Option<usize>,
) -> Option<usize> {
    let index = selected?;
    let mut next_selected = None;
    mutate_device(store, |buffer| {
        with_board_pins_mut(buffer, board, |pins| {
            next_selected = remove_pin(pins, index);
        });
    });
    next_selected
}

fn update_pin_name(
    store: &mut AppStore,
    board: &str,
    index: usize,
    name: &str,
) -> Result<(), String> {
    if !name.is_empty()
        && (!name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '.')))
    {
        return Err("Pin must contain only letters, numbers, underscores, or dots.".to_owned());
    }
    let mut result = Ok(());
    mutate_device(store, |buffer| {
        with_board_pins_mut(buffer, board, |pins| {
            if !name.is_empty()
                && pins.iter().enumerate().any(|(candidate_index, pin)| {
                    candidate_index != index && pin.get("pin").and_then(Value::as_str) == Some(name)
                })
            {
                result = Err("Pin names must be unique within the active board.".to_owned());
                return;
            }
            if let Some(pin) = pins.get_mut(index).and_then(Value::as_object_mut) {
                pin.insert("pin".to_owned(), Value::String(name.to_owned()));
            }
        });
    });
    result
}

fn update_pin_alias(
    store: &mut AppStore,
    board: &str,
    index: usize,
    alias: &str,
) -> Result<(), String> {
    if !alias.is_empty() && !valid_alias(alias) {
        return Err("Pin alias must be alphanumeric or use underscores.".to_owned());
    }
    let mut result = Ok(());
    mutate_device(store, |buffer| {
        with_board_pins_mut(buffer, board, |pins| {
            if !alias.is_empty()
                && pins.iter().enumerate().any(|(candidate_index, pin)| {
                    candidate_index != index
                        && pin
                            .get("alias")
                            .and_then(Value::as_str)
                            .is_some_and(|value| value.eq_ignore_ascii_case(alias))
                })
            {
                result = Err("Pin aliases must be unique within the active board.".to_owned());
                return;
            }
            if let Some(pin) = pins.get_mut(index).and_then(Value::as_object_mut) {
                pin.insert("alias".to_owned(), Value::String(alias.to_owned()));
            }
        });
    });
    result
}

fn valid_alias(alias: &str) -> bool {
    if alias.chars().all(|character| character.is_ascii_digit()) {
        return true;
    }
    let mut chars = alias.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
        && !alias.ends_with('_')
        && !alias.contains("__")
}

fn change_pin_type(
    store: &mut AppStore,
    board: &str,
    index: usize,
    next_type: &str,
) -> Result<(), String> {
    if !PIN_TYPES.iter().any(|(value, _)| value == &next_type) {
        return Err(format!("Unknown pin type {next_type:?}."));
    }
    mutate_device(store, |buffer| {
        with_board_pins_mut(buffer, board, |pins| {
            change_pin_type_in_place(pins, index, next_type)
        });
    });
    Ok(())
}

fn find_pin_index_by_name(store: &AppStore, board: &str, pin_name: &str) -> Option<usize> {
    let content = store.current_content();
    let buffer = serde_json::from_str::<Value>(&content).ok()?;
    board_pins(&buffer, board)
        .iter()
        .position(|pin| pin.get("pin").and_then(Value::as_str) == Some(pin_name))
}

fn add_pin(pins: &mut Vec<Value>, selected: Option<usize>) -> usize {
    let default_type = "digitalInput";
    let reference = selected.and_then(|index| pins.get(index)).cloned();
    if let (Some(index), Some(reference)) = (selected, reference) {
        let reference_type = reference
            .get("pinType")
            .and_then(Value::as_str)
            .unwrap_or(default_type);
        let reference_address = reference
            .get("address")
            .and_then(Value::as_str)
            .unwrap_or_else(|| address_prefix(reference_type));
        let candidate = increment_address(reference_address, reference_type);
        if !pins
            .iter()
            .any(|pin| pin.get("address").and_then(Value::as_str) == Some(candidate.as_str()))
        {
            pins.insert(index + 1, pin_value(reference_type, candidate));
            return index + 1;
        }
        let address = increment_address(&highest_address(pins, reference_type), reference_type);
        let insert_at = pins
            .iter()
            .rposition(|pin| pin.get("pinType").and_then(Value::as_str) == Some(reference_type))
            .map_or(pins.len(), |index| index + 1);
        pins.insert(insert_at, pin_value(reference_type, address));
        return insert_at;
    }
    let address = increment_address(&highest_address(pins, default_type), default_type);
    pins.push(pin_value(default_type, address));
    pins.len() - 1
}

fn remove_pin(pins: &mut Vec<Value>, index: usize) -> Option<usize> {
    let pin = pins.get(index)?.clone();
    let pin_type = pin
        .get("pinType")
        .and_then(Value::as_str)
        .unwrap_or("digitalInput")
        .to_owned();
    let address = pin
        .get("address")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let removed_position = address_position(&address, &pin_type);
    pins.remove(index);
    for pin in pins.iter_mut() {
        if pin.get("pinType").and_then(Value::as_str) == Some(pin_type.as_str()) {
            let current = pin
                .get("address")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if address_position(&current, &pin_type) > removed_position {
                if let Some(previous) = decrement_address(&current, &pin_type) {
                    if let Some(object) = pin.as_object_mut() {
                        object.insert("address".to_owned(), Value::String(previous));
                    }
                }
            }
        }
    }
    if pins.is_empty() {
        None
    } else if index >= pins.len() {
        Some(pins.len() - 1)
    } else {
        Some(index)
    }
}

fn change_pin_type_in_place(pins: &mut Vec<Value>, index: usize, next_type: &str) {
    let Some(mut pin) = pins.get(index).cloned() else {
        return;
    };
    let old_type = pin
        .get("pinType")
        .and_then(Value::as_str)
        .unwrap_or("digitalInput")
        .to_owned();
    if old_type == next_type {
        return;
    }
    let old_address = pin
        .get("address")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let removed_position = address_position(&old_address, &old_type);
    pins.remove(index);
    for candidate in pins.iter_mut() {
        if candidate.get("pinType").and_then(Value::as_str) == Some(old_type.as_str()) {
            let address = candidate
                .get("address")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if address_position(&address, &old_type) > removed_position {
                if let Some(previous) = decrement_address(&address, &old_type) {
                    if let Some(object) = candidate.as_object_mut() {
                        object.insert("address".to_owned(), Value::String(previous));
                    }
                }
            }
        }
    }
    let next_address = increment_address(&highest_address(pins, next_type), next_type);
    if let Some(object) = pin.as_object_mut() {
        object.insert("pinType".to_owned(), Value::String(next_type.to_owned()));
        object.insert("address".to_owned(), Value::String(next_address));
    }
    pins.push(pin);
    pins.sort_by(pin_sort);
}

fn pin_sort(left: &Value, right: &Value) -> Ordering {
    let left_type = left
        .get("pinType")
        .and_then(Value::as_str)
        .unwrap_or("digitalInput");
    let right_type = right
        .get("pinType")
        .and_then(Value::as_str)
        .unwrap_or("digitalInput");
    pin_type_rank(left_type)
        .cmp(&pin_type_rank(right_type))
        .then_with(|| {
            address_position(
                left.get("address")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                left_type,
            )
            .cmp(&address_position(
                right
                    .get("address")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
                right_type,
            ))
        })
}

fn pin_type_rank(pin_type: &str) -> usize {
    PIN_TYPES
        .iter()
        .position(|(value, _)| value == &pin_type)
        .unwrap_or(PIN_TYPES.len())
}

fn pin_value(pin_type: &str, address: String) -> Value {
    json!({ "pin": "", "pinType": pin_type, "address": address, "alias": "" })
}

fn address_prefix(pin_type: &str) -> &'static str {
    match pin_type {
        "digitalOutput" => "%QX",
        "analogInput" => "%IW",
        "analogOutput" => "%QW",
        _ => "%IX",
    }
}

fn highest_address(pins: &[Value], pin_type: &str) -> String {
    pins.iter()
        .filter(|pin| pin.get("pinType").and_then(Value::as_str) == Some(pin_type))
        .filter_map(|pin| pin.get("address").and_then(Value::as_str))
        .max_by_key(|address| address_position(address, pin_type))
        .map(str::to_owned)
        .unwrap_or_else(|| address_prefix(pin_type).to_owned())
}

fn address_position(address: &str, pin_type: &str) -> u64 {
    let rest = address
        .strip_prefix(address_prefix(pin_type))
        .unwrap_or(address);
    if matches!(pin_type, "digitalInput" | "digitalOutput") {
        let (byte, bit) = rest.split_once('.').unwrap_or((rest, "0"));
        let byte = byte.parse::<u64>().unwrap_or(0);
        let bit = bit.parse::<u64>().unwrap_or(0).min(7);
        byte * 8 + bit
    } else {
        rest.parse::<u64>().unwrap_or(0)
    }
}

fn increment_address(address: &str, pin_type: &str) -> String {
    let prefix = address_prefix(pin_type);
    let rest = address.strip_prefix(prefix).unwrap_or(address);
    if matches!(pin_type, "digitalInput" | "digitalOutput") {
        if rest.is_empty() {
            return format!("{prefix}0.0");
        }
        let (byte, bit) = rest.split_once('.').unwrap_or((rest, "0"));
        let byte = byte.parse::<u64>().unwrap_or(0);
        let bit = bit.parse::<u64>().unwrap_or(0);
        if bit >= 7 {
            format!("{prefix}{}.0", byte + 1)
        } else {
            format!("{prefix}{byte}.{}", bit + 1)
        }
    } else {
        let index = rest.parse::<u64>().ok();
        format!("{prefix}{}", index.map_or(0, |index| index + 1))
    }
}

fn decrement_address(address: &str, pin_type: &str) -> Option<String> {
    let prefix = address_prefix(pin_type);
    let rest = address.strip_prefix(prefix).unwrap_or(address);
    if matches!(pin_type, "digitalInput" | "digitalOutput") {
        let (byte, bit) = rest.split_once('.').unwrap_or((rest, "0"));
        let byte = byte.parse::<u64>().ok()?;
        let bit = bit.parse::<u64>().ok()?;
        if byte == 0 && bit == 0 {
            None
        } else if bit == 0 {
            Some(format!("{prefix}{}.7", byte - 1))
        } else {
            Some(format!("{prefix}{byte}.{}", bit - 1))
        }
    } else {
        let index = rest.parse::<u64>().ok()?;
        (index > 0).then(|| format!("{prefix}{}", index - 1))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        add_pin, board_pins, change_pin_type_in_place, increment_address,
        pin_mapping_enabled_for_known_state, remove_pin, switch_device_board,
    };

    #[test]
    fn board_switch_archives_vendor_data_and_preserves_per_board_pins() {
        let mut buffer = json!({
            "configuration": {
                "deviceBoard":"Board A",
                "communicationPort":"/dev/a",
                "vendorScreenData":{"slots":[1]},
                "vendorScreenDataByBoard":{"Board B":{"slots":[2]}},
                "selectedPlatformOptions":{"cpu":"old"}
            },
            "pinMapping": {
                "Board A":[{"pin":"1","pinType":"digitalInput","address":"%IX0.0","alias":"a"}],
                "Board B":[{"pin":"2","pinType":"digitalOutput","address":"%QX0.0","alias":"b"}]
            }
        });
        switch_device_board(&mut buffer, "Board B");
        assert_eq!(buffer["configuration"]["deviceBoard"], "Board B");
        assert_eq!(
            buffer["configuration"]["vendorScreenData"],
            json!({"slots":[2]})
        );
        assert_eq!(
            buffer["configuration"]["vendorScreenDataByBoard"]["Board A"],
            json!({"slots":[1]})
        );
        assert_eq!(
            buffer["configuration"]["selectedPlatformOptions"],
            json!({})
        );
        assert_eq!(board_pins(&buffer, "Board A").len(), 1);
        assert_eq!(board_pins(&buffer, "Board B").len(), 1);
    }

    #[test]
    fn pin_address_allocator_matches_pinned_prefix_and_rollover_rules() {
        assert_eq!(increment_address("%IX", "digitalInput"), "%IX0.0");
        assert_eq!(increment_address("%IX0.7", "digitalInput"), "%IX1.0");
        assert_eq!(increment_address("%QX2.3", "digitalOutput"), "%QX2.4");
        assert_eq!(increment_address("%IW", "analogInput"), "%IW0");
        assert_eq!(increment_address("%QW7", "analogOutput"), "%QW8");
    }

    #[test]
    fn add_remove_and_type_change_keep_type_address_spaces_compact() {
        let mut pins = Vec::new();
        let first = add_pin(&mut pins, None);
        assert_eq!(first, 0);
        assert_eq!(pins[0]["address"], "%IX0.0");
        let second = add_pin(&mut pins, Some(first));
        assert_eq!(second, 1);
        assert_eq!(pins[1]["address"], "%IX0.1");
        let third = add_pin(&mut pins, Some(second));
        assert_eq!(third, 2);
        assert_eq!(pins[2]["address"], "%IX0.2");

        change_pin_type_in_place(&mut pins, 1, "digitalOutput");
        assert!(pins
            .iter()
            .any(|pin| { pin["pinType"] == "digitalOutput" && pin["address"] == "%QX0.0" }));
        let digital_inputs = pins
            .iter()
            .filter(|pin| pin["pinType"] == "digitalInput")
            .map(|pin| pin["address"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(digital_inputs, vec!["%IX0.0", "%IX0.1"]);

        let input_index = pins
            .iter()
            .position(|pin| pin["pinType"] == "digitalInput" && pin["address"] == "%IX0.0")
            .unwrap();
        remove_pin(&mut pins, input_index);
        assert!(pins
            .iter()
            .any(|pin| { pin["pinType"] == "digitalInput" && pin["address"] == "%IX0.0" }));
    }

    #[test]
    fn built_in_targets_hide_empty_pin_mapping_but_imported_targets_show_it() {
        assert!(!pin_mapping_enabled_for_known_state(
            "OpenPLC Simulator",
            &[],
            None
        ));
        assert!(!pin_mapping_enabled_for_known_state(
            "OpenPLC Runtime v4",
            &[],
            None
        ));
        assert!(pin_mapping_enabled_for_known_state(
            "Vendor Board",
            &[],
            None
        ));
        assert!(pin_mapping_enabled_for_known_state(
            "OpenPLC Runtime v4",
            &[json!({"pin":"1"})],
            None
        ));
    }
}
