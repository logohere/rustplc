use dioxus::prelude::*;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::app::AppStore;

const FUNCTION_CODES: &[(&str, &str)] = &[
    ("1", "01 Read Coils"),
    ("2", "02 Read Discrete Inputs"),
    ("3", "03 Read Holding Registers"),
    ("4", "04 Read Input Registers"),
    ("5", "05 Write Single Coil"),
    ("6", "06 Write Single Register"),
    ("15", "15 Write Multiple Coils"),
    ("16", "16 Write Multiple Registers"),
];

#[component]
pub fn RemoteDeviceEditor(tab_name: String) -> Element {
    let mut store = use_context::<AppStore>();
    let Ok(device) = serde_json::from_str::<Value>(&store.current_content()) else {
        return invalid("Invalid remote device JSON.");
    };
    let name = text(&device, "name");
    let protocol = text(&device, "protocol");
    rsx! {
        div { class:"flex h-full w-full flex-col gap-4 overflow-y-auto pr-1",
            div { class:"flex items-center gap-4 rounded-lg border border-neutral-200 bg-neutral-50 p-3 dark:border-neutral-800 dark:bg-neutral-900",
                label { class:"font-caption text-xs font-medium", "Name" }
                input { class:"h-8 w-72 rounded border border-neutral-300 bg-white px-2 font-mono text-xs outline-none focus:border-brand dark:border-neutral-800 dark:bg-neutral-950", value:"{name}", oninput:move |event| set_root_string(&mut store,"name",event.value()) }
                span { class:"rounded bg-brand-light px-2 py-1 font-caption text-[10px] font-medium uppercase text-brand-dark", "{protocol}" }
            }
            match protocol.as_str() {
                "modbus-tcp" => rsx!{ ModbusRemote { device } },
                "ethercat" => rsx!{ EthercatRemote { device } },
                _ => rsx!{ div { class:"rounded border border-amber-200 bg-amber-50 p-3 font-caption text-xs text-amber-700 dark:border-amber-900 dark:bg-amber-950/20 dark:text-amber-300", "The pinned editor does not expose a complete configuration surface for {protocol}; the file is preserved." } },
            }
            span { class:"hidden", "{tab_name}" }
        }
    }
}

#[component]
fn ModbusRemote(device: Value) -> Element {
    let mut store = use_context::<AppStore>();
    let config = device
        .get("modbusTcpConfig")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let transport = text_default(&config, "transport", "tcp");
    let parity = text_default(&config, "parity", "N");
    let groups = config
        .get("ioGroups")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rsx! {
        Section { title:"Connection",
            div { class:"grid grid-cols-2 gap-3 lg:grid-cols-4",
                label { class:"flex flex-col gap-1 font-caption text-[10px] text-neutral-500", "Transport" select { class:"h-8 rounded border border-neutral-300 bg-white px-2 text-xs dark:border-neutral-800 dark:bg-neutral-950", value:"{transport}", onchange:move |event| set_path(&mut store,&["modbusTcpConfig","transport"],Value::String(event.value())), option {value:"tcp","TCP/IP"} option {value:"rtu","RTU serial"} } }
                if transport == "rtu" {
                    CompactText { label:"Serial Port", value:text_default(&config,"serialPort",""), path:vec!["modbusTcpConfig","serialPort"] }
                    CompactNumber { label:"Baud Rate", value:int_at(&config,&["baudRate"],9600), min:300,max:4_000_000,path:vec!["modbusTcpConfig","baudRate"] }
                    label { class:"flex flex-col gap-1 font-caption text-[10px] text-neutral-500",
                        "Parity"
                        select {
                            class:"h-8 rounded border border-neutral-300 bg-white px-2 text-xs dark:border-neutral-800 dark:bg-neutral-950",
                            value:"{parity}",
                            onchange:move |event| set_path(&mut store,&["modbusTcpConfig","parity"],Value::String(event.value())),
                            option{value:"N","None"}
                            option{value:"E","Even"}
                            option{value:"O","Odd"}
                        }
                    }
                    CompactNumber { label:"Stop Bits", value:int_at(&config,&["stopBits"],1), min:1,max:2,path:vec!["modbusTcpConfig","stopBits"] }
                    CompactNumber { label:"Data Bits", value:int_at(&config,&["dataBits"],8), min:7,max:8,path:vec!["modbusTcpConfig","dataBits"] }
                } else {
                    CompactText { label:"Host", value:text_default(&config,"host","127.0.0.1"), path:vec!["modbusTcpConfig","host"] }
                    CompactNumber { label:"Port", value:int_at(&config,&["port"],502), min:1,max:65535,path:vec!["modbusTcpConfig","port"] }
                }
                CompactNumber { label:"Slave ID", value:int_at(&config,&["slaveId"],1), min:0,max:255,path:vec!["modbusTcpConfig","slaveId"] }
                CompactNumber { label:"Timeout ms", value:int_at(&config,&["timeout"],1000), min:1,max:600_000,path:vec!["modbusTcpConfig","timeout"] }
            }
        }
        Section { title:"I/O Groups",
            div { class:"mb-2 flex justify-end", button { r#type:"button", class:"h-7 rounded bg-brand px-3 font-caption text-[10px] font-medium text-white", onclick:move |_| add_modbus_group(&mut store), "+ Group" } }
            div { class:"flex flex-col gap-3", for (index,group) in groups.iter().enumerate() {
                ModbusGroup { key:"group-{index}", index, group:group.clone() }
            } if groups.is_empty() { p { class:"py-6 text-center font-caption text-xs text-neutral-500", "No Modbus I/O groups." } } }
        }
    }
}

#[component]
fn ModbusGroup(index: usize, group: Value) -> Element {
    let mut store = use_context::<AppStore>();
    let points = group
        .get("ioPoints")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let fc = text_default(&group, "functionCode", "3");
    let error_handling = text_default(&group, "errorHandling", "keep-last-value");
    rsx! {
        div { class: "rounded-lg border border-neutral-200 p-3 dark:border-neutral-800",
            div { class: "grid grid-cols-2 gap-2 lg:grid-cols-7",
                CellText { label: "Name", value: text_default(&group,"name","Group"), path: vec!["modbusTcpConfig","ioGroups"], index, field: "name" }
                label { class: "flex flex-col gap-1 font-caption text-[9px] text-neutral-500",
                    "Function"
                    select {
                        class: "h-8 rounded border border-neutral-300 bg-white px-1 text-[10px] dark:border-neutral-800 dark:bg-neutral-950",
                        value: "{fc}",
                        onchange: move |event| {
                            set_array_nested_string(&mut store, &["modbusTcpConfig","ioGroups"], index, &["functionCode"], event.value());
                            recalc_modbus_group_points(&mut store, index);
                        },
                        for (code, label) in FUNCTION_CODES { option { value: "{code}", "{label}" } }
                    }
                }
                CellText { label:"Offset", value:text_default(&group,"offset","0"), path:vec!["modbusTcpConfig","ioGroups"], index, field:"offset" }
                CellNumber { label:"Length", value:int_at(&group,&["length"],1), min:1, max:2000, path:vec!["modbusTcpConfig","ioGroups"], index, field:"length" }
                CellNumber { label:"Cycle ms", value:int_at(&group,&["cycleTime"],100), min:1, max:600_000, path:vec!["modbusTcpConfig","ioGroups"], index, field:"cycleTime" }
                label { class:"flex flex-col gap-1 font-caption text-[9px] text-neutral-500",
                    "On Error"
                    select {
                        class:"h-8 rounded border border-neutral-300 bg-white px-1 text-[10px] dark:border-neutral-800 dark:bg-neutral-950",
                        value:"{error_handling}",
                        onchange:move |event| set_array_nested_string(&mut store,&["modbusTcpConfig","ioGroups"],index,&["errorHandling"],event.value()),
                        option { value:"keep-last-value", "Keep last" }
                        option { value:"set-to-zero", "Set zero" }
                    }
                }
                div { class:"flex items-end justify-end",
                    button { r#type:"button", class:"h-8 px-2 text-red-500", onclick:move |_| remove_array_path(&mut store,&["modbusTcpConfig","ioGroups"],index), "Delete" }
                }
            }
            div { class:"mt-3 flex items-center justify-between",
                h4 { class:"font-caption text-[10px] font-semibold", "IEC I/O Points" }
                button { r#type:"button", class:"h-7 rounded border border-brand px-2 text-[9px] text-brand", onclick:move |_| add_modbus_point(&mut store,index), "+ Point" }
            }
            div { class:"mt-1 overflow-x-auto",
                table { class:"w-full min-w-[720px] table-fixed border-collapse font-caption text-[10px]",
                    thead { tr { th {"Name"} th {"Type"} th {"IEC Location"} th {"Alias"} th {""} } }
                    tbody {
                        for (point_index, point) in points.iter().enumerate() {
                            {
                                let name = text_default(point, "name", "Point");
                                let point_type = text(point, "type");
                                let location = text(point, "iecLocation");
                                let alias = text(point, "alias");
                                rsx! {
                                    tr { key:"p-{index}-{point_index}", class:"border-t border-neutral-100 dark:border-neutral-900",
                                        td { PointText { value:name, group:index, point:point_index, field:"name" } }
                                        td { div { class:"px-2", "{point_type}" } }
                                        td { input { class:"h-7 w-full bg-transparent px-2 font-mono text-[10px] outline-none", value:"{location}", oninput:move |event| set_point_string(&mut store,index,point_index,"iecLocation",event.value()) } }
                                        td { PointText { value:alias, group:index, point:point_index, field:"alias" } }
                                        td { button { r#type:"button", class:"text-red-500", onclick:move |_| remove_modbus_point(&mut store,index,point_index), "×" } }
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

#[component]
fn EthercatRemote(device: Value) -> Element {
    let mut store = use_context::<AppStore>();
    let config = device
        .get("ethercatConfig")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let master = config
        .get("masterConfig")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let devices = config
        .get("devices")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rsx! {
        Section { title:"EtherCAT Master",
            Check { label:"Enabled", checked:master.get("enabled").and_then(Value::as_bool).unwrap_or(true), onchange:move |v| set_path(&mut store,&["ethercatConfig","masterConfig","enabled"],Value::Bool(v)) }
            div{class:"grid grid-cols-2 gap-3 lg:grid-cols-4",
                CompactText{label:"Interface",value:text_default(&master,"networkInterface","eth0"),path:vec!["ethercatConfig","masterConfig","networkInterface"]}
                CompactNumber{label:"Cycle µs",value:int_at(&master,&["cycleTimeUs"],1000),min:100,max:100000,path:vec!["ethercatConfig","masterConfig","cycleTimeUs"]}
                CompactNumber{label:"Watchdog Cycles",value:int_at(&master,&["watchdogTimeoutCycles"],3),min:1,max:100,path:vec!["ethercatConfig","masterConfig","watchdogTimeoutCycles"]}
                CompactNumber{label:"RT Priority",value:int_at(&master,&["taskPriority"],90),min:1,max:99,path:vec!["ethercatConfig","masterConfig","taskPriority"]}
            }
        }
        Section { title:"Configured EtherCAT Slaves",
            p { class:"mb-2 font-caption text-[10px] text-neutral-500", "ESI repository/scan metadata is preserved in each slave. This editor exposes identity, addressing, watchdog/DC settings, PDO/channel mappings and SDO startup values; live network scanning is a separate target-service operation." }
            div { class:"mb-2 flex justify-end", button { r#type:"button", class:"h-7 rounded bg-brand px-3 text-[10px] text-white", onclick:move |_| push_array_path(&mut store,&["ethercatConfig","devices"],default_ethercat_slave()), "+ Manual Slave" } }
            div { class:"flex flex-col gap-3", for (index,slave) in devices.iter().enumerate() { EthercatSlave { key:"slave-{index}", index, slave:slave.clone() } } if devices.is_empty(){p{class:"py-6 text-center font-caption text-xs text-neutral-500","No EtherCAT slaves configured."}} }
        }
    }
}

#[component]
fn EthercatSlave(index: usize, slave: Value) -> Element {
    let mut store = use_context::<AppStore>();
    let mappings = slave
        .get("channelMappings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rsx! {div{class:"rounded-lg border border-neutral-200 p-3 dark:border-neutral-800",
        div{class:"grid grid-cols-2 gap-2 lg:grid-cols-5",CellText{label:"Name",value:text_default(&slave,"name","Slave"),path:vec!["ethercatConfig","devices"],index,field:"name"} CellText{label:"Vendor ID",value:text_default(&slave,"vendorId","0x00000000"),path:vec!["ethercatConfig","devices"],index,field:"vendorId"} CellText{label:"Product",value:text_default(&slave,"productCode","0x00000000"),path:vec!["ethercatConfig","devices"],index,field:"productCode"} CellText{label:"Revision",value:text_default(&slave,"revisionNo","0x00000000"),path:vec!["ethercatConfig","devices"],index,field:"revisionNo"} div{class:"flex items-end justify-end",button{r#type:"button",class:"h-8 text-red-500",onclick:move |_|remove_array_path(&mut store,&["ethercatConfig","devices"],index),"Delete"}}}
        div{class:"mt-2 grid grid-cols-2 gap-2 lg:grid-cols-4", NestedNumber{label:"EtherCAT Address",value:int_at(&slave,&["config","addressing","ethercatAddress"],index as i64+1),min:0,max:65535,path:vec!["ethercatConfig","devices"],index,nested:vec!["config","addressing","ethercatAddress"]} NestedNumber{label:"SDO Timeout",value:int_at(&slave,&["config","timeouts","sdoTimeoutMs"],1000),min:0,max:600000,path:vec!["ethercatConfig","devices"],index,nested:vec!["config","timeouts","sdoTimeoutMs"]} NestedCheck{label:"SM Watchdog",checked:bool_at(&slave,&["config","watchdog","smWatchdogEnabled"],false),path:vec!["ethercatConfig","devices"],index,nested:vec!["config","watchdog","smWatchdogEnabled"]} NestedCheck{label:"Distributed Clocks",checked:bool_at(&slave,&["config","distributedClocks","dcEnabled"],false),path:vec!["ethercatConfig","devices"],index,nested:vec!["config","distributedClocks","dcEnabled"]}}
        div{class:"mt-3 flex items-center justify-between",h4{class:"font-caption text-[10px] font-semibold","Channel Mappings"}button{r#type:"button",class:"h-7 rounded border border-brand px-2 text-[9px] text-brand",onclick:move |_|push_nested_array(&mut store,&["ethercatConfig","devices"],index,"channelMappings",json!({"channelId":"","iecLocation":"","alias":""})),"+ Mapping"}}
        for (mapping_index,mapping) in mappings.iter().enumerate(){ {let channel=text(mapping,"channelId");let loc=text(mapping,"iecLocation");let alias=text(mapping,"alias");rsx!{div{key:"m-{index}-{mapping_index}",class:"mt-1 grid grid-cols-[1fr_1fr_1fr_auto] gap-1",input{class:"h-7 rounded border border-neutral-200 bg-transparent px-2 font-mono text-[10px] dark:border-neutral-800",placeholder:"channelId",value:"{channel}",oninput:move|event|set_double_nested_string(&mut store,&["ethercatConfig","devices"],index,"channelMappings",mapping_index,"channelId",event.value())}input{class:"h-7 rounded border border-neutral-200 bg-transparent px-2 font-mono text-[10px] dark:border-neutral-800",placeholder:"%IX0.0",value:"{loc}",oninput:move|event|set_double_nested_string(&mut store,&["ethercatConfig","devices"],index,"channelMappings",mapping_index,"iecLocation",event.value())}input{class:"h-7 rounded border border-neutral-200 bg-transparent px-2 font-mono text-[10px] dark:border-neutral-800",placeholder:"alias",value:"{alias}",oninput:move|event|set_double_nested_string(&mut store,&["ethercatConfig","devices"],index,"channelMappings",mapping_index,"alias",event.value())}button{r#type:"button",class:"text-red-500",onclick:move |_|remove_nested_array(&mut store,&["ethercatConfig","devices"],index,"channelMappings",mapping_index),"×"}}}}}
    }}
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
fn Check(label: &'static str, checked: bool, onchange: EventHandler<bool>) -> Element {
    rsx! {
        label { class: "mb-2 flex items-center gap-2 font-caption text-xs",
            input { r#type: "checkbox", checked, onchange: move |event| onchange.call(event.checked()) }
            "{label}"
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
fn CellText(
    label: &'static str,
    value: String,
    path: Vec<&'static str>,
    index: usize,
    field: &'static str,
) -> Element {
    let mut store = use_context::<AppStore>();
    rsx! {
        label { class: "flex flex-col gap-1 font-caption text-[9px] text-neutral-500",
            "{label}"
            input {
                class: "h-8 rounded border border-neutral-200 bg-transparent px-2 font-mono text-[10px] outline-none dark:border-neutral-800",
                value: "{value}",
                oninput: move |event| set_array_nested_string(&mut store, &path, index, &[field], event.value()),
            }
        }
    }
}

#[component]
fn CellNumber(
    label: &'static str,
    value: i64,
    min: i64,
    max: i64,
    path: Vec<&'static str>,
    index: usize,
    field: &'static str,
) -> Element {
    let mut store = use_context::<AppStore>();
    rsx! {
        label { class: "flex flex-col gap-1 font-caption text-[9px] text-neutral-500",
            "{label}"
            input {
                r#type: "number",
                min: "{min}",
                max: "{max}",
                class: "h-8 rounded border border-neutral-200 bg-transparent px-2 font-mono text-[10px] dark:border-neutral-800",
                value: "{value}",
                oninput: move |event| if let Ok(value) = event.value().parse::<i64>() {
                    set_array_nested_value(&mut store, &path, index, &[field], Value::from(value.clamp(min, max)));
                },
            }
        }
    }
}

#[component]
fn PointText(value: String, group: usize, point: usize, field: &'static str) -> Element {
    let mut store = use_context::<AppStore>();
    rsx! {
        input {
            class: "h-7 w-full bg-transparent px-2 font-mono text-[10px] outline-none",
            value: "{value}",
            oninput: move |event| set_point_string(&mut store, group, point, field, event.value()),
        }
    }
}

#[component]
fn NestedNumber(
    label: &'static str,
    value: i64,
    min: i64,
    max: i64,
    path: Vec<&'static str>,
    index: usize,
    nested: Vec<&'static str>,
) -> Element {
    let mut store = use_context::<AppStore>();
    rsx! {
        label { class: "flex flex-col gap-1 font-caption text-[9px] text-neutral-500",
            "{label}"
            input {
                r#type: "number",
                min: "{min}",
                max: "{max}",
                class: "h-8 rounded border border-neutral-200 bg-transparent px-2 font-mono text-[10px] dark:border-neutral-800",
                value: "{value}",
                oninput: move |event| if let Ok(value) = event.value().parse::<i64>() {
                    set_array_nested_value(&mut store, &path, index, &nested, Value::from(value.clamp(min, max)));
                },
            }
        }
    }
}

#[component]
fn NestedCheck(
    label: &'static str,
    checked: bool,
    path: Vec<&'static str>,
    index: usize,
    nested: Vec<&'static str>,
) -> Element {
    let mut store = use_context::<AppStore>();
    rsx! {
        label { class: "flex items-center gap-2 font-caption text-[10px]",
            input {
                r#type: "checkbox",
                checked,
                onchange: move |event| set_array_nested_value(&mut store, &path, index, &nested, Value::Bool(event.checked())),
            }
            "{label}"
        }
    }
}

fn add_modbus_group(store: &mut AppStore) {
    mutate(store, |root| {
        if let Some(rows) = ensure_array_path(root, &["modbusTcpConfig", "ioGroups"]) {
            rows.push(json!({"id":Uuid::new_v4().to_string(),"name":"Group","functionCode":"3","cycleTime":100,"offset":"0","length":1,"errorHandling":"keep-last-value","ioPoints":[]}));
        }
    });
}
fn add_modbus_point(store: &mut AppStore, group: usize) {
    let content = store.current_content();
    let Ok(mut root) = serde_json::from_str::<Value>(&content) else {
        return;
    };
    let fc = root
        .get("modbusTcpConfig")
        .and_then(|c| c.get("ioGroups"))
        .and_then(Value::as_array)
        .and_then(|groups| groups.get(group))
        .and_then(|g| g.get("functionCode"))
        .and_then(Value::as_str)
        .unwrap_or("3")
        .to_owned();
    let (point_type, prefix, is_bit) = fc_info(&fc);
    let location = next_location(store, &prefix, is_bit);
    if let Some(groups) = root
        .get_mut("modbusTcpConfig")
        .and_then(|c| c.get_mut("ioGroups"))
        .and_then(Value::as_array_mut)
    {
        if let Some(group_value) = groups.get_mut(group) {
            if !group_value.is_object() {
                *group_value = json!({});
            }
            let Some(object) = group_value.as_object_mut() else {
                return;
            };
            let points = object
                .entry("ioPoints".to_owned())
                .or_insert_with(|| Value::Array(Vec::new()));
            if let Some(points) = points.as_array_mut() {
                let n = points.len() + 1;
                points.push(json!({"id":Uuid::new_v4().to_string(),"name":format!("Point{n}"),"type":point_type,"iecLocation":location,"alias":""}));
                group_value["length"] = Value::from(points.len() as i64);
            }
        }
    }
    if let Ok(s) = serde_json::to_string_pretty(&root) {
        store.set_current_content(s);
    }
}
fn remove_modbus_point(store: &mut AppStore, group: usize, point: usize) {
    mutate(store, |root| {
        if let Some(group_value) = root
            .get_mut("modbusTcpConfig")
            .and_then(|c| c.get_mut("ioGroups"))
            .and_then(Value::as_array_mut)
            .and_then(|groups| groups.get_mut(group))
        {
            if let Some(points) = group_value
                .get_mut("ioPoints")
                .and_then(Value::as_array_mut)
            {
                if point < points.len() {
                    points.remove(point);
                }
                group_value["length"] = Value::from(points.len().max(1) as i64);
            }
        }
    });
}
fn recalc_modbus_group_points(store: &mut AppStore, group: usize) {
    let content = store.current_content();
    let Ok(mut root) = serde_json::from_str::<Value>(&content) else {
        return;
    };
    let fc = root
        .get("modbusTcpConfig")
        .and_then(|c| c.get("ioGroups"))
        .and_then(Value::as_array)
        .and_then(|g| g.get(group))
        .and_then(|g| g.get("functionCode"))
        .and_then(Value::as_str)
        .unwrap_or("3")
        .to_owned();
    let (point_type, prefix, is_bit) = fc_info(&fc);
    if let Some(points) = root
        .get_mut("modbusTcpConfig")
        .and_then(|c| c.get_mut("ioGroups"))
        .and_then(Value::as_array_mut)
        .and_then(|g| g.get_mut(group))
        .and_then(|g| g.get_mut("ioPoints"))
        .and_then(Value::as_array_mut)
    {
        for (i, p) in points.iter_mut().enumerate() {
            p["type"] = Value::String(point_type.clone());
            p["iecLocation"] = Value::String(format_location(&prefix, is_bit, i));
        }
    }
    if let Ok(s) = serde_json::to_string_pretty(&root) {
        store.set_current_content(s);
    }
}
fn set_point_string(store: &mut AppStore, group: usize, point: usize, field: &str, value: String) {
    mutate(store, |root| {
        if let Some(p) = root
            .get_mut("modbusTcpConfig")
            .and_then(|c| c.get_mut("ioGroups"))
            .and_then(Value::as_array_mut)
            .and_then(|g| g.get_mut(group))
            .and_then(|g| g.get_mut("ioPoints"))
            .and_then(Value::as_array_mut)
            .and_then(|p| p.get_mut(point))
            .and_then(Value::as_object_mut)
        {
            p.insert(field.to_owned(), Value::String(value));
        }
    });
}
fn fc_info(fc: &str) -> (String, String, bool) {
    match fc {
        "1" => ("Digital Input (Coil Status)".into(), "%IX".into(), true),
        "2" => ("Digital Input (Discrete Input)".into(), "%IX".into(), true),
        "3" => (
            "Analog Input (Holding Register)".into(),
            "%IW".into(),
            false,
        ),
        "4" => ("Analog Input (Input Register)".into(), "%IW".into(), false),
        "5" | "15" => ("Digital Output (Coil)".into(), "%QX".into(), true),
        _ => ("Analog Output (Register)".into(), "%QW".into(), false),
    }
}
fn next_location(store: &AppStore, prefix: &str, is_bit: bool) -> String {
    let mut max = -1i64;
    if let Some(project) = (store.project)() {
        if let Some(devices) = project
            .project
            .data
            .get("remoteDevices")
            .and_then(Value::as_array)
        {
            for device in devices {
                for group in device
                    .get("modbusTcpConfig")
                    .and_then(|c| c.get("ioGroups"))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    for point in group
                        .get("ioPoints")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                    {
                        if let Some(loc) = point.get("iecLocation").and_then(Value::as_str) {
                            if loc.starts_with(prefix) {
                                max = max.max(location_index(loc, prefix, is_bit));
                            }
                        }
                    }
                }
            }
        }
    }
    format_location(prefix, is_bit, (max + 1).max(0) as usize)
}
fn location_index(location: &str, prefix: &str, is_bit: bool) -> i64 {
    let tail = location.strip_prefix(prefix).unwrap_or("");
    if is_bit {
        let (b, bit) = tail.split_once('.').unwrap_or((tail, "0"));
        b.parse::<i64>().unwrap_or(0) * 8 + bit.parse::<i64>().unwrap_or(0)
    } else {
        tail.parse::<i64>().unwrap_or(0)
    }
}
fn format_location(prefix: &str, is_bit: bool, index: usize) -> String {
    if is_bit {
        format!("{prefix}{}.{}", index / 8, index % 8)
    } else {
        format!("{prefix}{index}")
    }
}
fn invalid(message: &str) -> Element {
    rsx! {
        div {
            class: "rounded border border-red-200 bg-red-50 p-3 font-caption text-xs text-red-600 dark:border-red-900 dark:bg-red-950/20 dark:text-red-400",
            "{message}"
        }
    }
}

fn default_ethercat_slave() -> Value {
    json!({"id":Uuid::new_v4().to_string(),"name":"EtherCAT Slave","esiDeviceRef":{"repositoryItemId":"manual","deviceIndex":0},"vendorId":"0x00000000","productCode":"0x00000000","revisionNo":"0x00000000","addedFrom":"scan","config":{"startupChecks":{"checkVendorId":true,"checkProductCode":true},"addressing":{"ethercatAddress":0},"timeouts":{"sdoTimeoutMs":1000,"initToPreOpTimeoutMs":5000,"safeOpToOpTimeoutMs":5000},"watchdog":{"smWatchdogEnabled":false,"smWatchdogMs":100,"pdiWatchdogEnabled":false,"pdiWatchdogMs":100},"distributedClocks":{"dcEnabled":false,"dcSyncUnitCycleUs":1000,"dcSync0Enabled":false,"dcSync0CycleUs":1000,"dcSync0ShiftUs":0,"dcSync1Enabled":false,"dcSync1CycleUs":1000,"dcSync1ShiftUs":0}},"channelMappings":[],"channelInfo":[],"rxPdos":[],"txPdos":[],"sdoConfigurations":[]})
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
        if let Some(o) = root.as_object_mut() {
            o.insert(key.to_owned(), Value::String(value));
        }
    });
}
fn set_path(store: &mut AppStore, path: &[&str], value: Value) {
    mutate(store, |root| set_json_path(root, path, value));
}
fn set_json_path(root: &mut Value, path: &[&str], value: Value) {
    let Some((last, parents)) = path.split_last() else {
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
fn push_nested_array(store: &mut AppStore, path: &[&str], index: usize, key: &str, value: Value) {
    mutate(store, |root| {
        if let Some(row) = ensure_array_path(root, path).and_then(|rows| rows.get_mut(index)) {
            if !row.is_object() {
                *row = json!({});
            }
            let Some(object) = row.as_object_mut() else {
                return;
            };
            let arr = object
                .entry(key.to_owned())
                .or_insert_with(|| Value::Array(Vec::new()));
            if !arr.is_array() {
                *arr = Value::Array(Vec::new());
            }
            if let Some(items) = arr.as_array_mut() {
                items.push(value);
            }
        }
    });
}
fn remove_nested_array(
    store: &mut AppStore,
    path: &[&str],
    index: usize,
    key: &str,
    nested_index: usize,
) {
    mutate(store, |root| {
        if let Some(arr) = ensure_array_path(root, path)
            .and_then(|rows| rows.get_mut(index))
            .and_then(|r| r.get_mut(key))
            .and_then(Value::as_array_mut)
        {
            if nested_index < arr.len() {
                arr.remove(nested_index);
            }
        }
    });
}
fn set_double_nested_string(
    store: &mut AppStore,
    path: &[&str],
    index: usize,
    key: &str,
    nested_index: usize,
    field: &str,
    value: String,
) {
    mutate(store, |root| {
        if let Some(o) = ensure_array_path(root, path)
            .and_then(|rows| rows.get_mut(index))
            .and_then(|r| r.get_mut(key))
            .and_then(Value::as_array_mut)
            .and_then(|a| a.get_mut(nested_index))
            .and_then(Value::as_object_mut)
        {
            o.insert(field.to_owned(), Value::String(value));
        }
    });
}
fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}
fn text_default(v: &Value, key: &str, d: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or(d).to_owned()
}
fn int_at(v: &Value, path: &[&str], d: i64) -> i64 {
    walk(v, path).and_then(Value::as_i64).unwrap_or(d)
}
fn bool_at(v: &Value, path: &[&str], d: bool) -> bool {
    walk(v, path).and_then(Value::as_bool).unwrap_or(d)
}
fn walk<'a>(mut v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    for k in path {
        v = v.get(*k)?;
    }
    Some(v)
}
