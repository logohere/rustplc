use dioxus::prelude::*;

use crate::app::AppStore;
use crate::runtime_v4_client::{
    is_user_management_capable_runtime, RuntimeUser, RuntimeUserRole, UpdateUserParams,
    MIN_USER_MANAGEMENT_RUNTIME_VERSION,
};

#[component]
pub fn UserManagementEditor() -> Element {
    let store = use_context::<AppStore>();
    let users = use_signal(Vec::<RuntimeUser>::new);
    let current = use_signal(|| None::<RuntimeUser>);
    let mut error = use_signal(|| None::<String>);
    let mut busy = use_signal(|| false);
    let mut refresh = use_signal(|| 0u64);

    let mut create_open = use_signal(|| false);
    let mut create_username = use_signal(String::new);
    let mut create_password = use_signal(String::new);
    let mut create_role = use_signal(|| RuntimeUserRole::User);

    let mut edit_id = use_signal(|| None::<u64>);
    let mut edit_username = use_signal(String::new);
    let mut edit_password = use_signal(String::new);
    let mut edit_current_password = use_signal(String::new);
    let mut edit_role = use_signal(|| RuntimeUserRole::User);

    let connected = (store.runtime_connected)();
    let capable = is_user_management_capable_runtime((store.runtime_version)().as_deref());
    let refresh_tick = refresh();
    let load_client = store.runtime_client;
    let mut load_users = users;
    let mut load_current = current;
    let mut load_error = error;
    use_future(move || async move {
        let _ = refresh_tick;
        if !connected || !capable {
            load_users.set(Vec::new());
            load_current.set(None);
            return;
        }
        let Some(client) = load_client() else {
            return;
        };
        let result = tokio::task::spawn_blocking(move || {
            let users = client.list_users()?;
            let current = client.who_am_i()?;
            Ok::<_, String>((users, current))
        })
        .await;
        match result {
            Ok(Ok((next_users, next_current))) => {
                load_users.set(next_users);
                load_current.set(Some(next_current));
                load_error.set(None);
            }
            Ok(Err(message)) => load_error.set(Some(message)),
            Err(join) => load_error.set(Some(format!("Runtime user refresh failed: {join}"))),
        }
    });

    let current_user = current();
    let is_admin = current_user
        .as_ref()
        .is_some_and(|user| user.role == RuntimeUserRole::Admin);

    if !connected || !capable {
        return rsx! {
            div { class: "flex h-full w-full select-none flex-col items-center justify-center gap-2 p-8 text-center",
                h2 { class: "text-lg font-semibold text-neutral-1000 dark:text-white", "User Management" }
                p { class: "max-w-lg text-sm text-neutral-500 dark:text-neutral-400",
                    if !connected {
                        "Connect to a Runtime from Device Configuration to manage its users."
                    } else {
                        {format!("User Management requires Runtime {MIN_USER_MANAGEMENT_RUNTIME_VERSION} or newer.")}
                    }
                }
            }
        };
    }

    let runtime_version = (store.runtime_version)().unwrap_or_else(|| "unknown".to_owned());
    let selected_user = edit_id().and_then(|id| users().into_iter().find(|user| user.id == id));

    rsx! {
        div { class: "flex h-full w-full select-none flex-col overflow-auto p-6",
            div { class: "mb-5 flex items-start justify-between gap-4",
                div {
                    h2 { class: "text-xl font-semibold text-neutral-1000 dark:text-white", "User Management" }
                    p { class: "mt-1 text-sm text-neutral-600 dark:text-neutral-400", "Manage accounts that can log in to Runtime {runtime_version}." }
                }
                div { class: "flex items-center gap-2",
                    button {
                        r#type: "button",
                        title: "Refresh users",
                        class: "h-9 rounded-md border border-neutral-300 px-3 text-sm hover:bg-neutral-100 dark:border-neutral-700 dark:hover:bg-neutral-850",
                        onclick: move |_| refresh += 1,
                        "Refresh"
                    }
                    if is_admin {
                        button {
                            r#type: "button",
                            class: "h-9 rounded-md bg-brand px-3 text-sm font-medium text-white hover:opacity-90",
                            onclick: move |_| {
                                create_username.set(String::new());
                                create_password.set(String::new());
                                create_role.set(RuntimeUserRole::User);
                                create_open.set(true);
                            },
                            "New User"
                        }
                    }
                }
            }

            if let Some(message) = error() {
                div { role: "alert", class: "mb-4 rounded-md border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-800 dark:bg-red-950/30 dark:text-red-300", "{message}" }
            }

            if create_open() && is_admin {
                div { class: "mb-4 grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_140px_auto] gap-2 rounded-lg border border-neutral-200 p-3 dark:border-neutral-800",
                    input {
                        value: "{create_username}",
                        placeholder: "username",
                        class: input_class(),
                        oninput: move |event| create_username.set(event.value())
                    }
                    input {
                        r#type: "password",
                        value: "{create_password}",
                        placeholder: "password",
                        class: input_class(),
                        oninput: move |event| create_password.set(event.value())
                    }
                    select {
                        class: input_class(),
                        value: create_role().as_str(),
                        onchange: move |event| create_role.set(role_from_value(&event.value())),
                        option { value: "admin", "Admin" }
                        option { value: "user", "User" }
                    }
                    div { class: "flex gap-2",
                        button {
                            r#type: "button",
                            disabled: busy() || create_username().trim().is_empty() || create_password().is_empty(),
                            class: primary_button_class(),
                            onclick: {
                                let client_signal = store.runtime_client;
                                move |_| {
                                    let Some(client) = client_signal() else { return; };
                                    let username = create_username().trim().to_owned();
                                    let password = create_password();
                                    let role = create_role();
                                    busy.set(true);
                                    spawn(async move {
                                        let result = tokio::task::spawn_blocking(move || client.create_user(&username, &password, role)).await;
                                        match result {
                                            Ok(Ok(())) => {
                                                create_open.set(false);
                                                error.set(None);
                                                refresh += 1;
                                            }
                                            Ok(Err(message)) => error.set(Some(message)),
                                            Err(join) => error.set(Some(format!("Create user task failed: {join}"))),
                                        }
                                        busy.set(false);
                                    });
                                }
                            },
                            "Create"
                        }
                        button { r#type: "button", class: secondary_button_class(), onclick: move |_| create_open.set(false), "Cancel" }
                    }
                }
            }

            div { class: "overflow-hidden rounded-lg border border-neutral-200 dark:border-neutral-800",
                table { class: "w-full border-collapse text-sm",
                    thead {
                        tr { class: "border-b border-neutral-200 bg-neutral-50 text-left dark:border-neutral-800 dark:bg-neutral-900",
                            th { class: "px-4 py-2 font-medium", "Username" }
                            th { class: "px-4 py-2 font-medium", "Role" }
                            th { class: "w-40 px-4 py-2 text-right font-medium", "Actions" }
                        }
                    }
                    tbody {
                        for user in users() {
                            {
                                let is_self = current_user.as_ref().is_some_and(|me| me.id == user.id);
                                let can_edit = is_admin || is_self;
                                let can_delete = is_admin && !is_self;
                                let edit_user = user.clone();
                                let delete_user = user.clone();
                                rsx! {
                                    tr { key: "user-{user.id}", class: "border-b border-neutral-100 last:border-b-0 dark:border-neutral-850",
                                        td { class: "px-4 py-2",
                                            "{user.username}"
                                            if is_self { span { class: "ml-2 text-xs text-neutral-400", "(you)" } }
                                        }
                                        td { class: "px-4 py-2 capitalize", "{user.role.as_str()}" }
                                        td { class: "px-4 py-2",
                                            div { class: "flex justify-end gap-2",
                                                if can_edit {
                                                    button {
                                                        r#type: "button",
                                                        class: secondary_button_class(),
                                                        title: "Edit user",
                                                        onclick: move |_| {
                                                            edit_id.set(Some(edit_user.id));
                                                            edit_username.set(edit_user.username.clone());
                                                            edit_password.set(String::new());
                                                            edit_current_password.set(String::new());
                                                            edit_role.set(edit_user.role);
                                                        },
                                                        "Edit"
                                                    }
                                                }
                                                if can_delete {
                                                    button {
                                                        r#type: "button",
                                                        disabled: busy(),
                                                        class: danger_button_class(),
                                                        title: "Delete user",
                                                        onclick: {
                                                            let client_signal = store.runtime_client;
                                                            move |_| {
                                                                let Some(client) = client_signal() else { return; };
                                                                let id = delete_user.id;
                                                                busy.set(true);
                                                                spawn(async move {
                                                                    let result = tokio::task::spawn_blocking(move || client.delete_user(id)).await;
                                                                    match result {
                                                                        Ok(Ok(())) => { error.set(None); refresh += 1; }
                                                                        Ok(Err(message)) => error.set(Some(message)),
                                                                        Err(join) => error.set(Some(format!("Delete user task failed: {join}"))),
                                                                    }
                                                                    busy.set(false);
                                                                });
                                                            }
                                                        },
                                                        "Delete"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if users().is_empty() {
                    p { class: "px-4 py-3 text-sm text-neutral-500", "No users found." }
                }
            }

            if let Some(user) = selected_user {
                {
                    let is_self = current_user.as_ref().is_some_and(|me| me.id == user.id);
                    let show_role = is_admin && !is_self;
                    rsx! {
                        div { class: "mt-4 grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_minmax(0,1fr)_140px_auto] gap-2 rounded-lg border border-neutral-200 p-3 dark:border-neutral-800",
                            input { value: "{edit_username}", class: input_class(), oninput: move |event| edit_username.set(event.value()) }
                            input { r#type: "password", value: "{edit_password}", placeholder: "new password (optional)", class: input_class(), oninput: move |event| edit_password.set(event.value()) }
                            if is_self {
                                input { r#type: "password", value: "{edit_current_password}", placeholder: "current password", class: input_class(), oninput: move |event| edit_current_password.set(event.value()) }
                            } else {
                                div {}
                            }
                            if show_role {
                                select { class: input_class(), value: edit_role().as_str(), onchange: move |event| edit_role.set(role_from_value(&event.value())),
                                    option { value: "admin", "Admin" }
                                    option { value: "user", "User" }
                                }
                            } else {
                                div { class: "flex items-center px-2 text-xs text-neutral-500", "{user.role.as_str()}" }
                            }
                            div { class: "flex gap-2",
                                button {
                                    r#type: "button",
                                    disabled: busy() || edit_username().trim().is_empty(),
                                    class: primary_button_class(),
                                    onclick: {
                                        let client_signal = store.runtime_client;
                                        let mut runtime_connected = store.runtime_connected;
                                        let mut runtime_version = store.runtime_version;
                                        let mut runtime_client = store.runtime_client;
                                        move |_| {
                                            let Some(client) = client_signal() else { return; };
                                            let username = edit_username().trim().to_owned();
                                            let password = edit_password();
                                            let current_password = edit_current_password();
                                            if is_self && !password.is_empty() && current_password.is_empty() {
                                                error.set(Some("Your current password is required to change your password.".to_owned()));
                                                return;
                                            }
                                            let params = UpdateUserParams {
                                                username: (username != user.username).then_some(username),
                                                password: (!password.is_empty()).then_some(password.clone()),
                                                current_password: (is_self && !password.is_empty()).then_some(current_password),
                                                role: (show_role && edit_role() != user.role).then_some(edit_role()),
                                            };
                                            if params == UpdateUserParams::default() {
                                                error.set(Some("No changes to save.".to_owned()));
                                                return;
                                            }
                                            let own_password_change = is_self && params.password.is_some();
                                            let user_id = user.id;
                                            busy.set(true);
                                            spawn(async move {
                                                let result = tokio::task::spawn_blocking(move || client.update_user(user_id, &params)).await;
                                                match result {
                                                    Ok(Ok(())) => {
                                                        edit_id.set(None);
                                                        error.set(None);
                                                        if own_password_change {
                                                            if let Some(client) = runtime_client() { let _ = client.logout(); }
                                                            runtime_client.set(None);
                                                            runtime_connected.set(false);
                                                            runtime_version.set(None);
                                                        } else {
                                                            refresh += 1;
                                                        }
                                                    }
                                                    Ok(Err(message)) => error.set(Some(message)),
                                                    Err(join) => error.set(Some(format!("Update user task failed: {join}"))),
                                                }
                                                busy.set(false);
                                            });
                                        }
                                    },
                                    "Save"
                                }
                                button { r#type: "button", class: secondary_button_class(), onclick: move |_| edit_id.set(None), "Cancel" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn role_from_value(value: &str) -> RuntimeUserRole {
    if value.eq_ignore_ascii_case("admin") {
        RuntimeUserRole::Admin
    } else {
        RuntimeUserRole::User
    }
}

fn input_class() -> &'static str {
    "h-9 min-w-0 rounded-md border border-neutral-300 bg-white px-3 text-sm outline-none focus:border-brand dark:border-neutral-700 dark:bg-neutral-900"
}

fn primary_button_class() -> &'static str {
    "h-9 rounded-md bg-brand px-3 text-sm font-medium text-white disabled:opacity-40"
}

fn secondary_button_class() -> &'static str {
    "h-8 rounded-md border border-neutral-300 px-2 text-xs hover:bg-neutral-100 dark:border-neutral-700 dark:hover:bg-neutral-850"
}

fn danger_button_class() -> &'static str {
    "h-8 rounded-md border border-red-300 px-2 text-xs text-red-600 hover:bg-red-50 disabled:opacity-40 dark:border-red-800 dark:text-red-400 dark:hover:bg-red-950/30"
}
