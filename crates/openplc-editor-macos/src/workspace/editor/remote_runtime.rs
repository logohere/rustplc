use std::time::Duration;

use dioxus::prelude::*;

use crate::app::{AppStore, ConsoleEntry};
use crate::generated_project::generate_project;
use crate::remote_debug::RemoteDebugSession;
use crate::runtime_v4_bundle::build_runtime_v4_zip;
use crate::runtime_v4_client::{
    describe_incompatible_runtime, is_strucpp_compatible_runtime, DeployOutcome, RuntimeLogLevel,
    RuntimeStatus, RuntimeV4Client, UsersInfo,
};
use crate::vpp_package::VppPackageManager;

const STATUS_POLL_INTERVAL: Duration = Duration::from_secs(1);

#[component]
pub fn RemoteRuntimeSlot(ip: String, board: String, allow_insecure_tls: bool) -> Element {
    rsx! {
        RemoteRuntimePanel {
            key: "runtime-{board}-{ip}",
            ip,
            board,
            allow_insecure_tls,
        }
    }
}

#[component]
fn RemoteRuntimePanel(ip: String, board: String, allow_insecure_tls: bool) -> Element {
    let store = use_context::<AppStore>();
    let client = store.runtime_client;
    let connected = store.runtime_connected;
    let mut shared_version = store.runtime_version;
    let users_info = use_signal(|| None::<UsersInfo>);
    let mut status = use_signal(|| None::<RuntimeStatus>);
    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);
    let busy = use_signal(|| false);
    let error = use_signal(|| None::<String>);

    let poll_client = client;
    let poll_connected = connected;
    let mut poll_status = status;
    let mut poll_error = error;
    use_future(move || async move {
        loop {
            if poll_connected() {
                if let Some(client) = poll_client() {
                    let result = tokio::task::spawn_blocking(move || client.status(true)).await;
                    match result {
                        Ok(Ok(next)) => {
                            poll_status.set(Some(next));
                            poll_error.set(None);
                        }
                        Ok(Err(message)) => poll_error.set(Some(message)),
                        Err(error) => {
                            poll_error.set(Some(format!("Runtime status task failed: {error}")))
                        }
                    }
                }
            }
            tokio::time::sleep(STATUS_POLL_INTERVAL).await;
        }
    });

    let version = shared_version().unwrap_or_else(|| "unknown".to_owned());
    let runtime_status = status()
        .map(|status| status.status)
        .unwrap_or_else(|| "Disconnected".to_owned());
    let is_v4 = is_runtime_v4_target(&board);
    let v4_compatible = !is_v4 || is_strucpp_compatible_runtime(Some(version.as_str()));

    rsx! {
        div { class: "mt-3 flex flex-col gap-3 rounded-lg border border-neutral-200 bg-neutral-50 p-3 dark:border-neutral-800 dark:bg-neutral-900/50",
            div { class: "flex items-center justify-between gap-3",
                div {
                    h3 { class: "font-caption text-sm font-semibold text-neutral-950 dark:text-white", "Runtime Connection" }
                    p { class: "font-caption text-[10px] text-neutral-500", if allow_insecure_tls { "HTTPS port 8443 · TLS verification disabled" } else { "Verified HTTPS connection · port 8443" } }
                }
                span {
                    class: if connected() {
                        "rounded-full bg-green-500/10 px-2 py-1 font-caption text-[9px] font-semibold uppercase text-green-600 dark:text-green-400"
                    } else {
                        "rounded-full bg-neutral-200 px-2 py-1 font-caption text-[9px] font-semibold uppercase text-neutral-500 dark:bg-neutral-800"
                    },
                    if connected() { "Connected" } else { "Disconnected" }
                }
            }

            div { class: "grid grid-cols-[112px_minmax(0,1fr)] items-center gap-x-3 gap-y-2",
                span { class: "font-caption text-[10px] font-medium text-neutral-600 dark:text-neutral-400", "Endpoint" }
                span { class: "truncate font-mono text-[10px] text-neutral-800 dark:text-neutral-200", "https://{ip}:8443" }
                span { class: "font-caption text-[10px] font-medium text-neutral-600 dark:text-neutral-400", "Runtime Version" }
                span { class: "font-mono text-[10px] text-neutral-800 dark:text-neutral-200", "{version}" }
                span { class: "font-caption text-[10px] font-medium text-neutral-600 dark:text-neutral-400", "PLC Status" }
                span { class: "font-mono text-[10px] text-neutral-800 dark:text-neutral-200", "{runtime_status}" }
            }

            if ip.trim().is_empty() {
                div { class: "rounded border border-amber-200 bg-amber-50 px-2 py-1 font-caption text-[10px] text-amber-700 dark:border-amber-900 dark:bg-amber-950/20 dark:text-amber-300",
                    "Enter a Runtime IP address above before connecting."
                }
            } else {
                div { class: "flex flex-wrap items-center gap-2",
                    RuntimeButton {
                        label: if busy() { "Working…" } else { "Probe" },
                        disabled: busy(),
                        onclick: {
                            let ip = ip.clone();
                            let mut client = client;
                            let mut users_info = users_info;
                            let mut error = error;
                            let mut busy = busy;
                            move |_| {
                                busy.set(true);
                                let ip = ip.clone();
                                spawn(async move {
                                    let result = tokio::task::spawn_blocking(move || {
                                        let candidate = RuntimeV4Client::new_with_tls(&ip, allow_insecure_tls)?;
                                        let mut info = candidate.users_info()?;
                                        if let Ok(version) = candidate.runtime_version() {
                                            info.runtime_version = Some(version);
                                        }
                                        Ok::<_, String>((candidate, info))
                                    })
                                    .await;
                                    match result {
                                        Ok(Ok((candidate, info))) => {
                                            shared_version.set(info.runtime_version.clone());
                                            client.set(Some(candidate));
                                            users_info.set(Some(info));
                                            error.set(None);
                                        }
                                        Ok(Err(message)) => error.set(Some(message)),
                                        Err(join) => error.set(Some(format!("Runtime probe task failed: {join}"))),
                                    }
                                    busy.set(false);
                                });
                            }
                        }
                    }
                    if client().is_some() && !connected() {
                        input {
                            class: "h-8 w-32 rounded-md border border-neutral-200 bg-white px-2 font-mono text-[10px] outline-none focus:border-brand dark:border-neutral-800 dark:bg-neutral-950",
                            value: "{username}",
                            placeholder: "username",
                            oninput: move |event| username.set(event.value()),
                        }
                        input {
                            r#type: "password",
                            class: "h-8 w-32 rounded-md border border-neutral-200 bg-white px-2 font-mono text-[10px] outline-none focus:border-brand dark:border-neutral-800 dark:bg-neutral-950",
                            value: "{password}",
                            placeholder: "password",
                            oninput: move |event| password.set(event.value()),
                        }
                        if users_info().is_some_and(|info| !info.has_users) {
                            RuntimeButton {
                                label: "Create Admin",
                                disabled: busy() || username().trim().is_empty() || password().is_empty(),
                                onclick: {
                                    let mut client_signal = client;
                                    let username = username;
                                    let password = password;
                                    let mut error = error;
                                    let mut connected = connected;
                                    let mut busy = busy;
                                    move |_| {
                                        let Some(candidate) = client_signal() else { return; };
                                        let username_value = username();
                                        let password_value = password();
                                        busy.set(true);
                                        spawn(async move {
                                            let result = tokio::task::spawn_blocking(move || {
                                                candidate.create_first_user(&username_value, &password_value)?;
                                                candidate.login(&username_value, &password_value)?;
                                                let status = candidate.status(true)?;
                                                Ok::<_, String>((candidate, status))
                                            })
                                            .await;
                                            match result {
                                                Ok(Ok((candidate, next_status))) => {
                                                    client_signal.set(Some(candidate));
                                                    connected.set(true);
                                                    status.set(Some(next_status));
                                                    error.set(None);
                                                }
                                                Ok(Err(message)) => error.set(Some(message)),
                                                Err(join) => error.set(Some(format!("Runtime bootstrap task failed: {join}"))),
                                            }
                                            busy.set(false);
                                        });
                                    }
                                }
                            }
                        } else {
                            RuntimeButton {
                                label: "Login",
                                disabled: busy() || username().trim().is_empty() || password().is_empty(),
                                onclick: {
                                    let mut client_signal = client;
                                    let username = username;
                                    let password = password;
                                    let mut error = error;
                                    let mut connected = connected;
                                    let mut busy = busy;
                                    move |_| {
                                        let Some(candidate) = client_signal() else { return; };
                                        let username_value = username();
                                        let password_value = password();
                                        busy.set(true);
                                        spawn(async move {
                                            let result = tokio::task::spawn_blocking(move || {
                                                candidate.login(&username_value, &password_value)?;
                                                let status = candidate.status(true)?;
                                                Ok::<_, String>((candidate, status))
                                            })
                                            .await;
                                            match result {
                                                Ok(Ok((candidate, next_status))) => {
                                                    client_signal.set(Some(candidate));
                                                    connected.set(true);
                                                    status.set(Some(next_status));
                                                    error.set(None);
                                                }
                                                Ok(Err(message)) => error.set(Some(message)),
                                                Err(join) => error.set(Some(format!("Runtime login task failed: {join}"))),
                                            }
                                            busy.set(false);
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if connected() {
                div { class: "flex flex-wrap items-center gap-2 border-t border-neutral-200 pt-3 dark:border-neutral-800",
                    RuntimeButton {
                        label: "Start",
                        disabled: busy(),
                        onclick: runtime_simple_action(client, busy, error, status, RuntimeAction::Start)
                    }
                    RuntimeButton {
                        label: "Stop",
                        disabled: busy(),
                        onclick: runtime_simple_action(client, busy, error, status, RuntimeAction::Stop)
                    }
                    RuntimeButton {
                        label: "Refresh",
                        disabled: busy(),
                        onclick: runtime_simple_action(client, busy, error, status, RuntimeAction::Refresh)
                    }
                    if is_v4 {
                        RuntimeButton {
                            label: "Attach Debugger",
                            disabled: busy() || store.any_dirty(),
                            onclick: {
                                let project = store.project;
                                let client_signal = client;
                                let mut remote_debug = store.remote_debug;
                                let mut busy = busy;
                                let mut error = error;
                                let host = ip.clone();
                                move |_| {
                                    if store.any_dirty() {
                                        error.set(Some("Save all modified tabs before attaching the remote debugger.".to_owned()));
                                        return;
                                    }
                                    let Some(project) = project() else { return; };
                                    let Some(candidate) = client_signal() else { return; };
                                    busy.set(true);
                                    let host = host.clone();
                                    spawn(async move {
                                        let result = tokio::task::spawn_blocking(move || {
                                            let generated = generate_project(&project, "runtime-debug-attach.st")?;
                                            let token = candidate.access_token()?;
                                            RemoteDebugSession::connect(
                                                &host,
                                                &token,
                                                generated.debug_map,
                                                &generated.md5,
                                                allow_insecure_tls,
                                            )
                                        })
                                        .await;
                                        match result {
                                            Ok(Ok(session)) => {
                                                remote_debug.set(Some(session));
                                                error.set(None);
                                            }
                                            Ok(Err(message)) => error.set(Some(message)),
                                            Err(join) => error.set(Some(format!("Remote debugger task failed: {join}"))),
                                        }
                                        busy.set(false);
                                    });
                                }
                            }
                        }
                    }
                    RuntimeButton {
                        label: "Build & Deploy",
                        disabled: busy() || store.any_dirty() || !v4_compatible,
                        onclick: {
                            let project = store.project;
                            let mut console = store.console;
                            let client_signal = client;
                            let mut busy = busy;
                            let mut error = error;
                            let mut status = status;
                            let mut remote_debug = store.remote_debug;
                            let deploy_v4 = is_v4;
                            let deploy_host = ip.clone();
                            move |_| {
                                let deploy_host = deploy_host.clone();
                                if store.any_dirty() {
                                    error.set(Some("Save all modified tabs before deploying to the Runtime.".to_owned()));
                                    return;
                                }
                                let Some(project) = project() else { return; };
                                let Some(candidate) = client_signal() else { return; };
                                busy.set(true);
                                spawn(async move {
                                    let result = tokio::task::spawn_blocking(move || {
                                        if deploy_v4 {
                                            let version = candidate.runtime_version().ok();
                                            if !is_strucpp_compatible_runtime(version.as_deref()) {
                                                return Err(describe_incompatible_runtime(version.as_deref()));
                                            }
                                        }
                                        let generated = generate_project(
                                            &project,
                                            if deploy_v4 { "runtime-v4-upload.st" } else { "runtime-v3-upload.st" },
                                        )?;
                                        let mut logs = Vec::<(RuntimeLogLevel, String)>::new();
                                        let outcome = if deploy_v4 {
                                            let zip = build_runtime_v4_zip(&generated, &project)?;
                                            candidate.deploy_program(&zip, true, |level, message| {
                                                logs.push((level, message.to_owned()));
                                            })?
                                        } else {
                                            candidate.deploy_file(
                                                generated.program_st.as_bytes(),
                                                "program.st",
                                                "text/plain",
                                                true,
                                                |level, message| logs.push((level, message.to_owned())),
                                            )?
                                        };
                                        let next_status = candidate.status(true).ok();
                                        let debug_attach = if deploy_v4 && outcome == DeployOutcome::Started {
                                            let token = candidate.access_token()?;
                                            Some(RemoteDebugSession::connect(
                                                &deploy_host,
                                                &token,
                                                generated.debug_map,
                                                &generated.md5,
                                                allow_insecure_tls,
                                            ))
                                        } else {
                                            None
                                        };
                                        Ok::<_, String>((outcome, logs, next_status, debug_attach))
                                    })
                                    .await;
                                    match result {
                                        Ok(Ok((outcome, logs, next_status, debug_attach))) => {
                                            console.with_mut(|entries| {
                                                for (level, message) in logs {
                                                    entries.push(ConsoleEntry {
                                                        level: console_level(level),
                                                        message,
                                                    });
                                                }
                                                entries.push(ConsoleEntry {
                                                    level: if outcome == DeployOutcome::Started { "info" } else { "error" },
                                                    message: format!("Runtime deploy result: {outcome:?}"),
                                                });
                                            });
                                            if let Some(next_status) = next_status {
                                                status.set(Some(next_status));
                                            }
                                            if let Some(debug_attach) = debug_attach {
                                                match debug_attach {
                                                    Ok(session) => {
                                                        remote_debug.set(Some(session));
                                                        console.with_mut(|entries| {
                                                            entries.push(ConsoleEntry {
                                                                level: "info",
                                                                message: "Remote debugger attached and MD5 verified.".to_owned(),
                                                            });
                                                        });
                                                    }
                                                    Err(message) => {
                                                        remote_debug.set(None);
                                                        console.with_mut(|entries| {
                                                            entries.push(ConsoleEntry {
                                                                level: "warning",
                                                                message: format!("Runtime deployed, but remote debugger attach failed: {message}"),
                                                            });
                                                        });
                                                    }
                                                }
                                            }
                                            if outcome == DeployOutcome::Started {
                                                error.set(None);
                                            } else {
                                                error.set(Some(format!("Runtime deployment did not reach STARTED: {outcome:?}")));
                                            }
                                        }
                                        Ok(Err(message)) => error.set(Some(message)),
                                        Err(join) => error.set(Some(format!("Runtime deploy task failed: {join}"))),
                                    }
                                    busy.set(false);
                                });
                            }
                        }
                    }
                    if store.any_dirty() {
                        span { class: "font-caption text-[9px] text-amber-600 dark:text-amber-400", "Save changes before deploy" }
                    }
                    if is_v4 && !v4_compatible {
                        span { class: "font-caption text-[9px] text-red-600 dark:text-red-400", "{describe_incompatible_runtime(Some(version.as_str()))}" }
                    }
                    RuntimeButton {
                        label: "Disconnect",
                        disabled: busy(),
                        onclick: {
                            let mut client = client;
                            let mut connected = connected;
                            let mut status = status;
                            let mut remote_debug = store.remote_debug;
                            move |_| {
                                if let Some(candidate) = client() {
                                    let _ = candidate.logout();
                                }
                                if let Some(session) = remote_debug() {
                                    let _ = session.disconnect();
                                }
                                remote_debug.set(None);
                                client.set(None);
                                connected.set(false);
                                shared_version.set(None);
                                status.set(None);
                            }
                        }
                    }
                }
            }

            if let Some(message) = error() {
                div { class: "rounded border border-red-200 bg-red-50 px-2 py-1 font-caption text-[10px] text-red-600 dark:border-red-900 dark:bg-red-950/20 dark:text-red-400", "{message}" }
            }
        }
    }
}

fn is_runtime_v4_target(board: &str) -> bool {
    board == "OpenPLC Runtime v4"
        || VppPackageManager::new()
            .ok()
            .and_then(|manager| manager.board(board))
            .is_some_and(|candidate| candidate.device.target.target_type == "runtime-v4")
}

#[derive(Clone, Copy)]
enum RuntimeAction {
    Start,
    Stop,
    Refresh,
}

fn runtime_simple_action(
    client: Signal<Option<RuntimeV4Client>>,
    mut busy: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut status: Signal<Option<RuntimeStatus>>,
    action: RuntimeAction,
) -> EventHandler<MouseEvent> {
    EventHandler::new(move |_| {
        let Some(candidate) = client() else {
            return;
        };
        busy.set(true);
        spawn(async move {
            let result = tokio::task::spawn_blocking(move || match action {
                RuntimeAction::Start => {
                    candidate.start_plc()?;
                    candidate.status(true)
                }
                RuntimeAction::Stop => {
                    candidate.stop_plc()?;
                    candidate.status(true)
                }
                RuntimeAction::Refresh => candidate.status(true),
            })
            .await;
            match result {
                Ok(Ok(next_status)) => {
                    status.set(Some(next_status));
                    error.set(None);
                }
                Ok(Err(message)) => error.set(Some(message)),
                Err(join) => error.set(Some(format!("Runtime action task failed: {join}"))),
            }
            busy.set(false);
        });
    })
}

#[component]
fn RuntimeButton(
    #[props(into)] label: String,
    disabled: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: "h-8 rounded-md border border-neutral-200 bg-white px-3 font-caption text-[10px] font-medium text-neutral-700 shadow-sm hover:border-brand hover:text-brand disabled:cursor-not-allowed disabled:opacity-40 dark:border-neutral-800 dark:bg-neutral-950 dark:text-neutral-200",
            disabled,
            onclick: move |event| onclick.call(event),
            "{label}"
        }
    }
}

fn console_level(level: RuntimeLogLevel) -> &'static str {
    match level {
        RuntimeLogLevel::Info => "info",
        RuntimeLogLevel::Warning => "warning",
        RuntimeLogLevel::Error => "error",
        RuntimeLogLevel::Debug => "info",
    }
}
