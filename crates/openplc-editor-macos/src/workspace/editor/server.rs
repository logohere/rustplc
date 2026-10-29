use dioxus::prelude::*;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::app::AppStore;

#[component]
pub fn ServerEditor(tab_name: String) -> Element {
    let mut store = use_context::<AppStore>();
    let Ok(server) = serde_json::from_str::<Value>(&store.current_content()) else {
        return invalid("Invalid server JSON.");
    };
    let name = text(&server, "name");
    let protocol = text(&server, "protocol");
    rsx! {
        div { class: "flex h-full w-full flex-col gap-4 overflow-y-auto pr-1",
            Header { name, protocol: protocol.clone(), on_name: move |value| set_root_string(&mut store, "name", value) }
            match protocol.as_str() {
                "modbus-tcp" => rsx! { ModbusServer { server } },
                "s7comm" => rsx! { S7Server { server } },
                "opcua" => rsx! { OpcUaServer { server } },
                _ => rsx! {
                    div { class: "rounded border border-amber-200 bg-amber-50 p-3 font-caption text-xs text-amber-700 dark:border-amber-900 dark:bg-amber-950/20 dark:text-amber-300",
                        "The pinned editor does not expose a configuration surface for {protocol}. The file is preserved, but this protocol cannot be edited here."
                    }
                },
            }
            span { class: "hidden", "{tab_name}" }
        }
    }
}

#[component]
fn Header(name: String, protocol: String, on_name: EventHandler<String>) -> Element {
    rsx! {
        div { class: "flex shrink-0 items-center gap-4 rounded-lg border border-neutral-200 bg-neutral-50 p-3 dark:border-neutral-800 dark:bg-neutral-900",
            label { class: "font-caption text-xs font-medium", "Name" }
            input {
                class: "h-8 w-72 rounded-md border border-neutral-300 bg-white px-2 font-mono text-xs outline-none focus:border-brand dark:border-neutral-800 dark:bg-neutral-950",
                value: "{name}",
                oninput: move |event| on_name.call(event.value()),
            }
            span { class: "rounded bg-brand-light px-2 py-1 font-caption text-[10px] font-medium uppercase tracking-wide text-brand-dark", "{protocol}" }
        }
    }
}

#[component]
fn ModbusServer(server: Value) -> Element {
    let mut store = use_context::<AppStore>();
    let config = server
        .get("modbusSlaveConfig")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let mapping = config
        .get("bufferMapping")
        .cloned()
        .unwrap_or_else(default_modbus_buffer_mapping);
    rsx! {
        Section { title: "Server Configuration",
            CheckRow { label: "Enable Server", checked: bool_at(&config, &["enabled"], false), onchange: move |value| set_path(&mut store, &["modbusSlaveConfig","enabled"], Value::Bool(value)) }
            TextRow { label: "Network Interface", value: text_default(&config,"networkInterface","0.0.0.0"), onchange: move |value| set_path(&mut store, &["modbusSlaveConfig","networkInterface"], Value::String(value)) }
            NumberRow { label: "Port", value: int_at(&config,&["port"],502), min:1, max:65535, onchange: move |value| set_path(&mut store, &["modbusSlaveConfig","port"], Value::from(value)) }
        }
        Section { title: "Buffer Mapping",
            p { class: "mb-2 font-caption text-[10px] text-neutral-500", "Register counts max 1024; bit counts max 8192, matching the pinned editor/runtime buffer contract." }
            div { class: "grid grid-cols-2 gap-3 lg:grid-cols-4",
                BufferNumber { label:"%QW", value:int_at(&mapping,&["holdingRegisters","qwCount"],1024), max:1024, path:vec!["modbusSlaveConfig","bufferMapping","holdingRegisters","qwCount"] }
                BufferNumber { label:"%MW", value:int_at(&mapping,&["holdingRegisters","mwCount"],1024), max:1024, path:vec!["modbusSlaveConfig","bufferMapping","holdingRegisters","mwCount"] }
                BufferNumber { label:"%MD", value:int_at(&mapping,&["holdingRegisters","mdCount"],1024), max:1024, path:vec!["modbusSlaveConfig","bufferMapping","holdingRegisters","mdCount"] }
                BufferNumber { label:"%ML", value:int_at(&mapping,&["holdingRegisters","mlCount"],1024), max:1024, path:vec!["modbusSlaveConfig","bufferMapping","holdingRegisters","mlCount"] }
                BufferNumber { label:"%QX bits", value:int_at(&mapping,&["coils","qxBits"],8192), max:8192, path:vec!["modbusSlaveConfig","bufferMapping","coils","qxBits"] }
                BufferNumber { label:"%MX bits", value:int_at(&mapping,&["coils","mxBits"],0), max:8192, path:vec!["modbusSlaveConfig","bufferMapping","coils","mxBits"] }
                BufferNumber { label:"%IX bits", value:int_at(&mapping,&["discreteInputs","ixBits"],8192), max:8192, path:vec!["modbusSlaveConfig","bufferMapping","discreteInputs","ixBits"] }
                BufferNumber { label:"%IW", value:int_at(&mapping,&["inputRegisters","iwCount"],1024), max:1024, path:vec!["modbusSlaveConfig","bufferMapping","inputRegisters","iwCount"] }
            }
        }
    }
}

#[component]
fn BufferNumber(label: &'static str, value: i64, max: i64, path: Vec<&'static str>) -> Element {
    let mut store = use_context::<AppStore>();
    rsx! {
        label { class: "flex flex-col gap-1 font-caption text-[10px] font-medium text-neutral-600 dark:text-neutral-400",
            "{label}"
            input { r#type:"number", min:"0", max:"{max}", class:"h-8 rounded-md border border-neutral-300 bg-white px-2 font-mono text-xs outline-none focus:border-brand dark:border-neutral-800 dark:bg-neutral-950", value:"{value}", oninput: move |event| if let Ok(value)=event.value().parse::<i64>() { set_path(&mut store,&path,Value::from(value.clamp(0,max))); } }
        }
    }
}

#[component]
fn S7Server(server: Value) -> Element {
    let mut store = use_context::<AppStore>();
    let config = server
        .get("s7commSlaveConfig")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let settings = config.get("server").cloned().unwrap_or_else(|| json!({}));
    let identity = config
        .get("plcIdentity")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let logging = config.get("logging").cloned().unwrap_or_else(|| json!({}));
    let blocks = config
        .get("dataBlocks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rsx! {
        Section { title:"Server Settings",
            CheckRow { label:"Enable Server", checked:bool_at(&settings,&["enabled"],false), onchange:move |v| set_path(&mut store,&["s7commSlaveConfig","server","enabled"],Value::Bool(v)) }
            div { class:"grid grid-cols-2 gap-3 lg:grid-cols-4",
                CompactText { label:"Bind Address", value:text_default(&settings,"bindAddress","0.0.0.0"), path:vec!["s7commSlaveConfig","server","bindAddress"] }
                CompactNumber { label:"Port", value:int_at(&settings,&["port"],102), min:1,max:65535,path:vec!["s7commSlaveConfig","server","port"] }
                CompactNumber { label:"Max Clients", value:int_at(&settings,&["maxClients"],32), min:1,max:1024,path:vec!["s7commSlaveConfig","server","maxClients"] }
                CompactNumber { label:"PDU Size", value:int_at(&settings,&["pduSize"],480), min:240,max:960,path:vec!["s7commSlaveConfig","server","pduSize"] }
                CompactNumber { label:"Work ms", value:int_at(&settings,&["workIntervalMs"],100), min:1,max:10000,path:vec!["s7commSlaveConfig","server","workIntervalMs"] }
                CompactNumber { label:"Send Timeout", value:int_at(&settings,&["sendTimeoutMs"],3000), min:100,max:60000,path:vec!["s7commSlaveConfig","server","sendTimeoutMs"] }
                CompactNumber { label:"Receive Timeout", value:int_at(&settings,&["recvTimeoutMs"],3000), min:100,max:60000,path:vec!["s7commSlaveConfig","server","recvTimeoutMs"] }
                CompactNumber { label:"Ping Timeout", value:int_at(&settings,&["pingTimeoutMs"],10000), min:1000,max:300000,path:vec!["s7commSlaveConfig","server","pingTimeoutMs"] }
            }
        }
        Section { title:"PLC Identity",
            div { class:"grid grid-cols-2 gap-3 lg:grid-cols-3",
                CompactText { label:"Name", value:text_default(&identity,"name","OpenPLC Runtime"), path:vec!["s7commSlaveConfig","plcIdentity","name"] }
                CompactText { label:"Module Type", value:text_default(&identity,"moduleType","CPU 315-2 PN/DP"), path:vec!["s7commSlaveConfig","plcIdentity","moduleType"] }
                CompactText { label:"Serial Number", value:text_default(&identity,"serialNumber","S C-OPENPLC01"), path:vec!["s7commSlaveConfig","plcIdentity","serialNumber"] }
                CompactText { label:"Copyright", value:text_default(&identity,"copyright","OpenPLC Project"), path:vec!["s7commSlaveConfig","plcIdentity","copyright"] }
                CompactText { label:"Module Name", value:text_default(&identity,"moduleName","OpenPLC"), path:vec!["s7commSlaveConfig","plcIdentity","moduleName"] }
            }
        }
        Section { title:"Data Blocks",
            div { class:"mb-2 flex justify-end",
                button { r#type:"button", class:"h-7 rounded-md bg-brand px-3 font-caption text-[10px] font-medium text-white", onclick:move |_| push_array_path(&mut store,&["s7commSlaveConfig","dataBlocks"],json!({"dbNumber":1,"description":"","sizeBytes":256,"mapping":{"type":"int_memory","startBuffer":0,"bitAddressing":false}})), "+ Data Block" }
            }
            div { class:"overflow-x-auto rounded border border-neutral-200 dark:border-neutral-800",
                table { class:"w-full min-w-[760px] table-fixed border-collapse font-caption text-xs",
                    thead { tr { th {"DB"} th {"Description"} th {"Size"} th {"Mapping"} th {"Start"} th {"Bits"} th {""} } }
                    tbody { for (index, block) in blocks.iter().enumerate() {
                        {
                            let db=int_at(block,&["dbNumber"],1); let description=text(block,"description"); let size=int_at(block,&["sizeBytes"],256); let mapping=text_default(&block["mapping"],"type","int_memory"); let start=int_at(&block["mapping"],&["startBuffer"],0); let bits=block["mapping"].get("bitAddressing").and_then(Value::as_bool).unwrap_or(false);
                            rsx! { tr { key:"db-{index}", class:"border-t border-neutral-100 dark:border-neutral-900",
                                td { CellNumber { value:db, min:1,max:65535, path:vec!["s7commSlaveConfig","dataBlocks"], index, field:"dbNumber" } }
                                td { CellString { value:description, path:vec!["s7commSlaveConfig","dataBlocks"], index, field:"description" } }
                                td { CellNumber { value:size, min:1,max:65536, path:vec!["s7commSlaveConfig","dataBlocks"], index, field:"sizeBytes" } }
                                td { select { class:"h-8 w-full bg-transparent px-1 text-xs outline-none", value:"{mapping}", onchange:move |event| set_array_nested_string(&mut store,&["s7commSlaveConfig","dataBlocks"],index,&["mapping","type"],event.value()), for choice in ["bool_input","bool_output","bool_memory","byte_input","byte_output","int_input","int_output","int_memory","dint_input","dint_output","dint_memory","lint_input","lint_output","lint_memory"] { option { value:"{choice}", "{choice}" } } } }
                                td { input { r#type:"number", class:"h-8 w-full bg-transparent px-2 font-mono text-xs outline-none", value:"{start}", oninput:move |event| if let Ok(v)=event.value().parse::<i64>() { set_array_nested_value(&mut store,&["s7commSlaveConfig","dataBlocks"],index,&["mapping","startBuffer"],Value::from(v.clamp(0,1023))); } } }
                                td { input { r#type:"checkbox", checked:bits, onchange:move |event| set_array_nested_value(&mut store,&["s7commSlaveConfig","dataBlocks"],index,&["mapping","bitAddressing"],Value::Bool(event.checked())) } }
                                td { button { r#type:"button", class:"text-red-500", onclick:move |_| remove_array_path(&mut store,&["s7commSlaveConfig","dataBlocks"],index), "×" } }
                            } }
                        }
                    } }
                }
            }
        }
        Section { title:"Logging",
            div { class:"flex gap-6",
                CheckRow { label:"Connections", checked:bool_at(&logging,&["logConnections"],true), onchange:move |v| set_path(&mut store,&["s7commSlaveConfig","logging","logConnections"],Value::Bool(v)) }
                CheckRow { label:"Data Access", checked:bool_at(&logging,&["logDataAccess"],false), onchange:move |v| set_path(&mut store,&["s7commSlaveConfig","logging","logDataAccess"],Value::Bool(v)) }
                CheckRow { label:"Errors", checked:bool_at(&logging,&["logErrors"],true), onchange:move |v| set_path(&mut store,&["s7commSlaveConfig","logging","logErrors"],Value::Bool(v)) }
            }
        }
    }
}

#[component]
fn OpcUaServer(server: Value) -> Element {
    let mut store = use_context::<AppStore>();
    let config = server
        .get("opcuaServerConfig")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let settings = config.get("server").cloned().unwrap_or_else(|| json!({}));
    let security = config.get("security").cloned().unwrap_or_else(|| json!({}));
    let profiles = config
        .get("securityProfiles")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let users = config
        .get("users")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let certificate_strategy =
        text_default(&security, "serverCertificateStrategy", "auto_self_signed");
    let custom_certificate = security
        .get("serverCertificateCustom")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let custom_private_key = security
        .get("serverPrivateKeyCustom")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let nodes = config
        .get("addressSpace")
        .and_then(|v| v.get("nodes"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rsx! {
        Section { title:"OPC-UA Server",
            CheckRow { label:"Enable Server", checked:bool_at(&settings,&["enabled"],false), onchange:move |v| set_path(&mut store,&["opcuaServerConfig","server","enabled"],Value::Bool(v)) }
            div { class:"grid grid-cols-2 gap-3 lg:grid-cols-3",
                CompactText { label:"Server Name", value:text_default(&settings,"name","OpenPLC OPC UA Server"), path:vec!["opcuaServerConfig","server","name"] }
                CompactText { label:"Bind Address", value:text_default(&settings,"bindAddress","0.0.0.0"), path:vec!["opcuaServerConfig","server","bindAddress"] }
                CompactNumber { label:"Port", value:int_at(&settings,&["port"],4840), min:1,max:65535,path:vec!["opcuaServerConfig","server","port"] }
                CompactText { label:"Endpoint Path", value:text_default(&settings,"endpointPath","/openplc/opcua"), path:vec!["opcuaServerConfig","server","endpointPath"] }
                CompactText { label:"Application URI", value:text_default(&settings,"applicationUri","urn:openplc:opcua:server"), path:vec!["opcuaServerConfig","server","applicationUri"] }
                CompactText { label:"Product URI", value:text_default(&settings,"productUri","urn:openplc:runtime"), path:vec!["opcuaServerConfig","server","productUri"] }
                CompactNumber { label:"Cycle Time ms", value:int_at(&config,&["cycleTimeMs"],100), min:10,max:10000,path:vec!["opcuaServerConfig","cycleTimeMs"] }
                CompactText { label:"Namespace URI", value:config.get("addressSpace").map(|v|text_default(v,"namespaceUri","urn:openplc:opcua:namespace")).unwrap_or_default(), path:vec!["opcuaServerConfig","addressSpace","namespaceUri"] }
            }
        }
        Section { title:"Security",
            select {
                class:"h-8 w-64 rounded border border-neutral-300 bg-white px-2 text-xs dark:border-neutral-800 dark:bg-neutral-950",
                value:"{certificate_strategy}",
                onchange:move |event| set_path(&mut store,&["opcuaServerConfig","security","serverCertificateStrategy"],Value::String(event.value())),
                option { value:"auto_self_signed", "Auto self-signed" }
                option { value:"custom", "Custom certificate" }
            }
            if certificate_strategy == "custom" {
                textarea {
                    class:"mt-2 h-28 w-full rounded border border-neutral-300 bg-transparent p-2 font-mono text-[10px] dark:border-neutral-800",
                    placeholder:"Server certificate PEM",
                    value:"{custom_certificate}",
                    oninput:move |event| set_path(&mut store,&["opcuaServerConfig","security","serverCertificateCustom"],Value::String(event.value()))
                }
                textarea {
                    class:"mt-2 h-28 w-full rounded border border-neutral-300 bg-transparent p-2 font-mono text-[10px] dark:border-neutral-800",
                    placeholder:"Private key PEM",
                    value:"{custom_private_key}",
                    oninput:move |event| set_path(&mut store,&["opcuaServerConfig","security","serverPrivateKeyCustom"],Value::String(event.value()))
                }
            }
        }
        Section { title:"Security Profiles",
            div { class:"mb-2 flex justify-end", button { r#type:"button", class:"h-7 rounded bg-brand px-3 text-[10px] text-white", onclick:move |_| push_array_path(&mut store,&["opcuaServerConfig","securityProfiles"],json!({"id":Uuid::new_v4().to_string(),"name":"profile","enabled":true,"securityPolicy":"None","securityMode":"None","authMethods":["Anonymous"]})), "+ Profile" } }
            SimpleJsonRows { rows:profiles, path:vec!["opcuaServerConfig","securityProfiles"], kind:"profile" }
        }
        Section { title:"Users",
            div { class:"mb-2 flex justify-end", button { r#type:"button", class:"h-7 rounded bg-brand px-3 text-[10px] text-white", onclick:move |_| push_array_path(&mut store,&["opcuaServerConfig","users"],json!({"id":Uuid::new_v4().to_string(),"type":"password","username":"user","passwordHash":null,"certificateId":null,"role":"viewer"})), "+ User" } }
            SimpleJsonRows { rows:users, path:vec!["opcuaServerConfig","users"], kind:"user" }
        }
        Section { title:"Address Space",
            p { class:"mb-2 font-caption text-[10px] text-neutral-500", "Node addresses are resolved from the compiler debug-map at build time; stored project datatypes are not trusted for runtime byte widths." }
            div { class:"mb-2 flex justify-end", button { r#type:"button", class:"h-7 rounded bg-brand px-3 text-[10px] text-white", onclick:move |_| push_array_path(&mut store,&["opcuaServerConfig","addressSpace","nodes"],json!({"id":Uuid::new_v4().to_string(),"pouName":"GVL","variablePath":"","variableType":"BOOL","nodeId":format!("ns=2;s={}",Uuid::new_v4()),"browseName":"Variable","displayName":"Variable","description":"","permissions":{"viewer":"r","operator":"rw","engineer":"rw"},"nodeType":"variable"})), "+ Node" } }
            OpcNodeRows { rows:nodes }
        }
    }
}

#[component]
fn SimpleJsonRows(rows: Vec<Value>, path: Vec<&'static str>, kind: &'static str) -> Element {
    let mut store = use_context::<AppStore>();
    rsx! {
        div { class: "flex flex-col gap-2",
            for (index, row) in rows.iter().enumerate() {
                {
                    let summary = serde_json::to_string(row).unwrap_or_default();
                    let path_for_edit = path.clone();
                    let path_for_remove = path.clone();
                    rsx! {
                        div {
                            key: "{kind}-{index}",
                            class: "flex items-center gap-2 rounded border border-neutral-200 p-2 dark:border-neutral-800",
                            input {
                                class: "min-w-0 flex-1 bg-transparent font-mono text-[10px] outline-none",
                                value: "{summary}",
                                onchange: move |event| if let Ok(value) = serde_json::from_str::<Value>(&event.value()) {
                                    set_array_value(&mut store, &path_for_edit, index, value);
                                },
                            }
                            button {
                                r#type: "button",
                                class: "text-red-500",
                                onclick: move |_| remove_array_path(&mut store, &path_for_remove, index),
                                "×"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn OpcNodeRows(rows: Vec<Value>) -> Element {
    let mut store = use_context::<AppStore>();
    rsx! {
        div { class: "overflow-x-auto rounded border border-neutral-200 dark:border-neutral-800",
            table { class: "w-full min-w-[1000px] table-fixed border-collapse font-caption text-xs",
                thead { tr { th {"POU"} th {"Variable Path"} th {"Node Type"} th {"Node ID"} th {"Browse Name"} th {"Display Name"} th {""} } }
                tbody {
                    for (index, row) in rows.iter().enumerate() {
                        {
                            let pou = text(row, "pouName");
                            let variable = text(row, "variablePath");
                            let node_type = text_default(row, "nodeType", "variable");
                            let node_id = text(row, "nodeId");
                            let browse = text(row, "browseName");
                            let display = text(row, "displayName");
                            rsx! {
                                tr { key: "opc-node-{index}", class: "border-t border-neutral-100 dark:border-neutral-900",
                                    td { NodeCell { value: pou, index, field: "pouName" } }
                                    td { NodeCell { value: variable, index, field: "variablePath" } }
                                    td {
                                        select {
                                            class: "h-8 w-full bg-transparent text-xs",
                                            value: "{node_type}",
                                            onchange: move |event| set_array_nested_string(&mut store, &["opcuaServerConfig","addressSpace","nodes"], index, &["nodeType"], event.value()),
                                            option { value: "variable", "variable" }
                                            option { value: "array", "array" }
                                            option { value: "structure", "structure" }
                                        }
                                    }
                                    td { NodeCell { value: node_id, index, field: "nodeId" } }
                                    td { NodeCell { value: browse, index, field: "browseName" } }
                                    td { NodeCell { value: display, index, field: "displayName" } }
                                    td { button { r#type: "button", class: "text-red-500", onclick: move |_| remove_array_path(&mut store, &["opcuaServerConfig","addressSpace","nodes"], index), "×" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NodeCell(value: String, index: usize, field: &'static str) -> Element {
    let mut store = use_context::<AppStore>();
    rsx! {input{class:"h-8 w-full bg-transparent px-2 font-mono text-[10px] outline-none",value:"{value}",oninput:move|event|set_array_nested_string(&mut store,&["opcuaServerConfig","addressSpace","nodes"],index,&[field],event.value())}}
}

#[component]
fn Section(title: &'static str, children: Element) -> Element {
    rsx! {
        div {
            class: "rounded-lg border border-neutral-200 bg-white p-4 dark:border-neutral-800 dark:bg-neutral-950",
            h3 { class: "mb-3 font-caption text-sm font-semibold text-neutral-950 dark:text-white", "{title}" }
            {children}
        }
    }
}

#[component]
fn CheckRow(label: &'static str, checked: bool, onchange: EventHandler<bool>) -> Element {
    rsx! {
        label { class: "flex items-center gap-3 font-caption text-xs",
            input { r#type: "checkbox", checked, onchange: move |event| onchange.call(event.checked()) }
            "{label}"
        }
    }
}

#[component]
fn TextRow(label: &'static str, value: String, onchange: EventHandler<String>) -> Element {
    rsx! {
        label { class: "flex items-center gap-3 font-caption text-xs",
            span { class: "w-36", "{label}" }
            input {
                class: "h-8 min-w-64 rounded border border-neutral-300 bg-white px-2 font-mono text-xs dark:border-neutral-800 dark:bg-neutral-950",
                value: "{value}",
                oninput: move |event| onchange.call(event.value()),
            }
        }
    }
}

#[component]
fn NumberRow(
    label: &'static str,
    value: i64,
    min: i64,
    max: i64,
    onchange: EventHandler<i64>,
) -> Element {
    rsx! {
        label { class: "flex items-center gap-3 font-caption text-xs",
            span { class: "w-36", "{label}" }
            input {
                r#type: "number",
                min: "{min}",
                max: "{max}",
                class: "h-8 w-36 rounded border border-neutral-300 bg-white px-2 font-mono text-xs dark:border-neutral-800 dark:bg-neutral-950",
                value: "{value}",
                oninput: move |event| if let Ok(value) = event.value().parse::<i64>() {
                    onchange.call(value.clamp(min, max));
                },
            }
        }
    }
}

#[component]
fn CompactText(label: &'static str, value: String, path: Vec<&'static str>) -> Element {
    let mut store = use_context::<AppStore>();
    rsx! {
        label { class: "flex flex-col gap-1 font-caption text-[10px] text-neutral-500",
            "{label}"
            input {
                class: "h-8 rounded border border-neutral-300 bg-white px-2 font-mono text-xs dark:border-neutral-800 dark:bg-neutral-950",
                value: "{value}",
                oninput: move |event| set_path(&mut store, &path, Value::String(event.value())),
            }
        }
    }
}

#[component]
fn CompactNumber(
    label: &'static str,
    value: i64,
    min: i64,
    max: i64,
    path: Vec<&'static str>,
) -> Element {
    let mut store = use_context::<AppStore>();
    rsx! {
        label { class: "flex flex-col gap-1 font-caption text-[10px] text-neutral-500",
            "{label}"
            input {
                r#type: "number",
                min: "{min}",
                max: "{max}",
                class: "h-8 rounded border border-neutral-300 bg-white px-2 font-mono text-xs dark:border-neutral-800 dark:bg-neutral-950",
                value: "{value}",
                oninput: move |event| if let Ok(value) = event.value().parse::<i64>() {
                    set_path(&mut store, &path, Value::from(value.clamp(min, max)));
                },
            }
        }
    }
}

#[component]
fn CellString(
    value: String,
    path: Vec<&'static str>,
    index: usize,
    field: &'static str,
) -> Element {
    let mut store = use_context::<AppStore>();
    rsx! {
        input {
            class: "h-8 w-full bg-transparent px-2 font-mono text-xs outline-none",
            value: "{value}",
            oninput: move |event| set_array_nested_string(&mut store, &path, index, &[field], event.value()),
        }
    }
}

#[component]
fn CellNumber(
    value: i64,
    min: i64,
    max: i64,
    path: Vec<&'static str>,
    index: usize,
    field: &'static str,
) -> Element {
    let mut store = use_context::<AppStore>();
    rsx! {
        input {
            r#type: "number",
            min: "{min}",
            max: "{max}",
            class: "h-8 w-full bg-transparent px-2 font-mono text-xs outline-none",
            value: "{value}",
            oninput: move |event| if let Ok(value) = event.value().parse::<i64>() {
                set_array_nested_value(&mut store, &path, index, &[field], Value::from(value.clamp(min, max)));
            },
        }
    }
}

fn invalid(message: &str) -> Element {
    rsx! {div{class:"rounded border border-red-200 bg-red-50 p-3 font-caption text-xs text-red-600 dark:border-red-900 dark:bg-red-950/20 dark:text-red-400","{message}"}}
}
fn default_modbus_buffer_mapping() -> Value {
    json!({"holdingRegisters":{"qwCount":1024,"mwCount":1024,"mdCount":1024,"mlCount":1024},"coils":{"qxBits":8192,"mxBits":0},"discreteInputs":{"ixBits":8192},"inputRegisters":{"iwCount":1024}})
}
fn mutate(store: &mut AppStore, f: impl FnOnce(&mut Value)) {
    let content = store.current_content();
    let Ok(mut value) = serde_json::from_str::<Value>(&content) else {
        return;
    };
    f(&mut value);
    if let Ok(content) = serde_json::to_string_pretty(&value) {
        store.set_current_content(content);
    }
}
fn set_root_string(store: &mut AppStore, key: &str, value: String) {
    mutate(store, |root| {
        if let Some(object) = root.as_object_mut() {
            object.insert(key.to_owned(), Value::String(value));
        }
    });
}
fn set_path(store: &mut AppStore, path: &[&str], value: Value) {
    mutate(store, |root| set_json_path(root, path, value));
}
fn set_json_path(root: &mut Value, path: &[&str], value: Value) {
    let Some((last, parents)) = path.split_last() else {
        *root = value;
        return;
    };
    let mut current = root;
    for key in parents {
        if !current.is_object() {
            *current = json!({});
        }
        let Some(object) = current.as_object_mut() else {
            return;
        };
        let entry = object.entry((*key).to_owned()).or_insert_with(|| json!({}));
        if !entry.is_object() {
            *entry = json!({});
        }
        current = entry;
    }
    if !current.is_object() {
        *current = json!({});
    }
    if let Some(object) = current.as_object_mut() {
        object.insert((*last).to_owned(), value);
    }
}
fn push_array_path(store: &mut AppStore, path: &[&str], value: Value) {
    mutate(store, |root| {
        if let Some(rows) = ensure_array_path(root, path) {
            rows.push(value);
        }
    });
}
fn remove_array_path(store: &mut AppStore, path: &[&str], index: usize) {
    mutate(store, |root| {
        let Some(rows) = ensure_array_path(root, path) else {
            return;
        };
        if index < rows.len() {
            rows.remove(index);
        }
    });
}
fn set_array_value(store: &mut AppStore, path: &[&str], index: usize, value: Value) {
    mutate(store, |root| {
        if let Some(row) = ensure_array_path(root, path).and_then(|rows| rows.get_mut(index)) {
            *row = value;
        }
    });
}
fn set_array_nested_string(
    store: &mut AppStore,
    path: &[&str],
    index: usize,
    nested: &[&str],
    value: String,
) {
    set_array_nested_value(store, path, index, nested, Value::String(value));
}
fn set_array_nested_value(
    store: &mut AppStore,
    path: &[&str],
    index: usize,
    nested: &[&str],
    value: Value,
) {
    mutate(store, |root| {
        if let Some(row) = ensure_array_path(root, path).and_then(|rows| rows.get_mut(index)) {
            set_json_path(row, nested, value);
        }
    });
}
fn ensure_array_path<'a>(root: &'a mut Value, path: &[&str]) -> Option<&'a mut Vec<Value>> {
    let (last, parents) = path.split_last()?;
    let mut current = root;
    for key in parents {
        if !current.is_object() {
            *current = json!({});
        }
        let object = current.as_object_mut()?;
        let entry = object.entry((*key).to_owned()).or_insert_with(|| json!({}));
        if !entry.is_object() {
            *entry = json!({});
        }
        current = entry;
    }
    if !current.is_object() {
        *current = json!({});
    }
    let object = current.as_object_mut()?;
    let entry = object
        .entry((*last).to_owned())
        .or_insert_with(|| Value::Array(Vec::new()));
    if !entry.is_array() {
        *entry = Value::Array(Vec::new());
    }
    entry.as_array_mut()
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
fn text_default(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_owned()
}
fn bool_at(value: &Value, path: &[&str], default: bool) -> bool {
    walk(value, path)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}
fn int_at(value: &Value, path: &[&str], default: i64) -> i64 {
    walk(value, path).and_then(Value::as_i64).unwrap_or(default)
}
fn walk<'a>(mut value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    for key in path {
        value = value.get(*key)?;
    }
    Some(value)
}
