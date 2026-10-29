use std::collections::HashSet;

use dioxus::prelude::*;
use open_rust_plc_compiler::frontend::ast::VarBlockType as CompilerVarBlockType;
use open_rust_plc_compiler::stdlib::iec_standard_fb_unit;
use serde_json::{json, Value};

use crate::app::AppStore;
use crate::plc_model::{
    GraphEdge, GraphMeasured, GraphNode, GraphViewport, ParsedPou, PlcVariable, PlcVariableType,
    PouBody, PouInterface, PouLanguage, PouType, VariableClass,
};

#[component]
pub fn GraphCanvas(
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
    width: f64,
    height: f64,
    viewport: GraphViewport,
    allow_pan_zoom: bool,
    on_viewport: EventHandler<GraphViewport>,
    on_move: EventHandler<(String, f64, f64)>,
    on_add: EventHandler<(String, f64, f64)>,
    on_delete: EventHandler<String>,
    on_delete_edge: EventHandler<String>,
    on_copy_selection: EventHandler<Vec<String>>,
    on_paste: EventHandler<()>,
    allow_edge_selection: bool,
    allow_handle_branch_drop: bool,
    on_handle_branch_drop: EventHandler<(String, String, String, String)>,
    on_connect: EventHandler<(String, String, String, String)>,
    on_edit: EventHandler<String>,
) -> Element {
    let mut selected_nodes = use_signal(HashSet::<String>::new);
    let mut selected_edge = use_signal(|| None::<String>);
    let mut pending_source = use_signal(|| None::<(String, String)>);
    let mut view = use_signal(move || viewport);
    let mut pan_origin = use_signal(|| None::<(f64, f64, GraphViewport)>);
    let current_view = view();
    let transform = format!(
        "position:absolute;left:0;top:0;width:{width}px;height:{height}px;transform-origin:0 0;transform:translate({}px,{}px) scale({});",
        current_view.x, current_view.y, current_view.zoom
    );
    let grid_size = 16.0 * current_view.zoom;
    let canvas_style = format!(
        "min-height:{height}px;height:{height}px;min-width:{width}px;width:{width}px;background-image:radial-gradient(circle,#d1d5db 1px,transparent 1px);background-size:{grid_size}px {grid_size}px;background-position:{}px {}px;",
        current_view.x, current_view.y
    );
    let zoom_label = format!("{:.0}%", current_view.zoom * 100.0);

    rsx! {
        div {
            class: if allow_pan_zoom {
                "relative overflow-hidden rounded-lg border border-neutral-200 bg-white outline-none focus:ring-1 focus:ring-brand dark:border-neutral-800 dark:bg-neutral-950"
            } else {
                "relative overflow-auto rounded-lg border border-neutral-200 bg-white outline-none focus:ring-1 focus:ring-brand dark:border-neutral-800 dark:bg-neutral-950"
            },
            style: canvas_style,
            tabindex: "0",
            onmousedown: move |event| {
                if allow_pan_zoom {
                    let point = event.client_coordinates();
                    pan_origin.set(Some((point.x, point.y, view())));
                }
            },
            onmousemove: move |event| {
                if let Some((start_x, start_y, start_view)) = pan_origin() {
                    let point = event.client_coordinates();
                    view.set(GraphViewport {
                        x: start_view.x + point.x - start_x,
                        y: start_view.y + point.y - start_y,
                        zoom: start_view.zoom,
                    });
                }
            },
            onmouseup: move |_| {
                if pan_origin().is_some() {
                    pan_origin.set(None);
                    on_viewport.call(view());
                }
            },
            onmouseleave: move |_| {
                if pan_origin().is_some() {
                    pan_origin.set(None);
                    on_viewport.call(view());
                }
            },
            onwheel: move |event| {
                if allow_pan_zoom {
                    event.prevent_default();
                    let delta = event.delta().strip_units();
                    if delta.y != 0.0 {
                        let point = event.element_coordinates();
                        let next = zoom_about(view(), if delta.y > 0.0 { 0.9 } else { 1.1 }, point.x, point.y);
                        view.set(next);
                        on_viewport.call(next);
                    }
                }
            },
            onclick: move |_| {
                selected_nodes.set(HashSet::new());
                selected_edge.set(None);
            },
            onkeydown: move |event| {
                let modifiers = event.modifiers();
                let command = modifiers.meta() || modifiers.ctrl();
                if command {
                    match event.key() {
                        Key::Character(value) if value.eq_ignore_ascii_case("c") => {
                            let nodes = selected_nodes().iter().cloned().collect::<Vec<_>>();
                            if !nodes.is_empty() {
                                event.prevent_default();
                                on_copy_selection.call(nodes);
                            }
                            return;
                        }
                        Key::Character(value) if value.eq_ignore_ascii_case("v") => {
                            event.prevent_default();
                            on_paste.call(());
                            return;
                        }
                        _ => {}
                    }
                }
                if matches!(event.key(), Key::Delete | Key::Backspace) {
                    let nodes = selected_nodes().iter().cloned().collect::<Vec<_>>();
                    if !nodes.is_empty() {
                        for id in nodes {
                            on_delete.call(id);
                        }
                        selected_nodes.set(HashSet::new());
                        pending_source.set(None);
                    } else if let Some(id) = selected_edge() {
                        on_delete_edge.call(id);
                        selected_edge.set(None);
                    }
                }
            },
            ondragover: move |event| event.prevent_default(),
            ondrop: move |event| {
                event.prevent_default();
                let transfer = event.data_transfer();
                let point = event.element_coordinates();
                let point = flow_point(point.x, point.y, view());
                if let Some(node_type) = transfer
                    .get_data("application/reactflow/fbd-blocks")
                    .filter(|value| !value.is_empty())
                    .or_else(|| {
                        transfer
                            .get_data("application/reactflow/ladder-blocks")
                            .filter(|value| !value.is_empty())
                    })
                {
                    on_add.call((node_type, snap_grid(point.0), snap_grid(point.1)));
                } else if let Some(id) = transfer.get_as_text() {
                    on_move.call((id, snap_grid(point.0), snap_grid(point.1)));
                }
            },
            div { style: transform,
                svg {
                class: if allow_edge_selection {
                    "absolute inset-0 h-full w-full overflow-visible"
                } else {
                    "pointer-events-none absolute inset-0 h-full w-full overflow-visible"
                },
                width: "{width}",
                height: "{height}",
                for edge in edges.iter() {
                    if let (Some(source), Some(target)) = (
                        nodes.iter().find(|node| node.id == edge.source),
                        nodes.iter().find(|node| node.id == edge.target),
                    ) {
                        {
                            let (sx, sy) = handle_point(source, &edge.source_handle, true);
                            let (tx, ty) = handle_point(target, &edge.target_handle, false);
                            let edge_selected = selected_edge().as_deref() == Some(edge.id.as_str());
                            let stroke = if edge_selected { "#EF4444" } else { "#0464FB" };
                            let stroke_width = if edge_selected { "3" } else { "1.5" };
                            let edge_id = edge.id.clone();
                            rsx! {
                                path {
                                    key: "{edge.id}",
                                    d: "M {sx} {sy} C {(sx + tx) / 2.0} {sy}, {(sx + tx) / 2.0} {ty}, {tx} {ty}",
                                    fill: "none",
                                    stroke,
                                    stroke_width,
                                    pointer_events: if allow_edge_selection { "stroke" } else { "none" },
                                    cursor: if allow_edge_selection { "pointer" } else { "default" },
                                    onmousedown: move |event| event.stop_propagation(),
                                    onclick: move |event| {
                                        if allow_edge_selection {
                                            event.stop_propagation();
                                            selected_nodes.set(HashSet::new());
                                            selected_edge.set(Some(edge_id.clone()));
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }
            for node in nodes.iter().cloned() {
                GraphNodeView {
                    key: "{node.id}",
                    node: node.clone(),
                    selected: selected_nodes().contains(&node.id),
                    pending_source: pending_source().as_ref().is_some_and(|(id, _)| id == &node.id),
                    on_select: move |(id, additive): (String, bool)| {
                        selected_edge.set(None);
                        if additive {
                            selected_nodes.with_mut(|selected| {
                                if !selected.insert(id.clone()) {
                                    selected.remove(&id);
                                }
                            });
                        } else {
                            selected_nodes.set(HashSet::from([id]));
                        }
                    },
                    on_delete: move |id: String| {
                        on_delete.call(id.clone());
                        selected_nodes.with_mut(|selected| {
                            selected.remove(&id);
                        });
                    },
                    allow_handle_branch_drop,
                    on_handle_branch_drop,
                    on_edit,
                    on_handle: move |(node_id, handle_id, kind): (String, String, String)| {
                        if kind == "source" {
                            pending_source.set(Some((node_id, handle_id)));
                            return;
                        }
                        if kind != "target" {
                            return;
                        }
                        if let Some((source_id, source_handle)) = pending_source() {
                            if source_id != node_id {
                                on_connect.call((source_id, source_handle, node_id, handle_id));
                            }
                            pending_source.set(None);
                        }
                    },
                }
            }
            }
            if allow_pan_zoom {
                div {
                    class: "absolute right-2 top-2 z-30 flex items-center gap-1 rounded-md border border-neutral-200 bg-white/95 p-1 shadow-sm dark:border-neutral-800 dark:bg-neutral-950/95",
                    onmousedown: move |event| event.stop_propagation(),
                    onclick: move |event| event.stop_propagation(),
                    button {
                        r#type: "button",
                        class: "h-6 w-6 rounded text-xs font-semibold hover:bg-neutral-100 dark:hover:bg-neutral-900",
                        title: "Zoom out",
                        onclick: move |_| {
                            let next = zoom_about(view(), 0.9, width / 2.0, height / 2.0);
                            view.set(next);
                            on_viewport.call(next);
                        },
                        "−"
                    }
                    span { class: "min-w-10 text-center font-mono text-[9px] text-neutral-500", "{zoom_label}" }
                    button {
                        r#type: "button",
                        class: "h-6 w-6 rounded text-xs font-semibold hover:bg-neutral-100 dark:hover:bg-neutral-900",
                        title: "Zoom in",
                        onclick: move |_| {
                            let next = zoom_about(view(), 1.1, width / 2.0, height / 2.0);
                            view.set(next);
                            on_viewport.call(next);
                        },
                        "+"
                    }
                    button {
                        r#type: "button",
                        class: "h-6 rounded px-2 text-[9px] font-medium hover:bg-neutral-100 dark:hover:bg-neutral-900",
                        title: "Reset pan and zoom",
                        onclick: move |_| {
                            let next = GraphViewport::default();
                            view.set(next);
                            on_viewport.call(next);
                        },
                        "Reset"
                    }
                }
            }
        }
    }
}

#[component]
fn GraphNodeView(
    node: GraphNode,
    selected: bool,
    pending_source: bool,
    on_select: EventHandler<(String, bool)>,
    on_delete: EventHandler<String>,
    allow_handle_branch_drop: bool,
    on_handle_branch_drop: EventHandler<(String, String, String, String)>,
    on_edit: EventHandler<String>,
    on_handle: EventHandler<(String, String, String)>,
) -> Element {
    let width = node_width(&node);
    let height = node_height(&node);
    let node_id = node.id.clone();
    let drag_id = node.id.clone();
    let border = if selected {
        "ring-2 ring-brand"
    } else if pending_source {
        "ring-2 ring-amber-400"
    } else {
        ""
    };
    let style = format!(
        "position:absolute;left:{}px;top:{}px;width:{}px;height:{}px;z-index:2;",
        node.position.x, node.position.y, width, height
    );

    rsx! {
        div {
            id: "{node.id}",
            class: "group select-none rounded {border}",
            style,
            draggable: node.draggable,
            onmousedown: move |event| event.stop_propagation(),
            onclick: move |event| {
                event.stop_propagation();
                if node.selectable {
                    let modifiers = event.modifiers();
                    on_select.call((
                        node_id.clone(),
                        modifiers.shift() || modifiers.ctrl() || modifiers.meta(),
                    ));
                }
            },
            ondoubleclick: {
                let edit_id = node.id.clone();
                move |event| {
                    event.stop_propagation();
                    if node.selectable && node.node_type != "powerRail" {
                        on_edit.call(edit_id.clone());
                    }
                }
            },
            ondragstart: move |event| {
                let transfer = event.data_transfer();
                let _ = transfer.set_data("text/plain", &drag_id);
                transfer.set_effect_allowed("move");
            },
            NodeVisual { node: node.clone() }
            for handle in node_handles(&node) {
                GraphHandle {
                    key: "{node.id}-{handle.id}-{handle.kind}",
                    node_id: node.id.clone(),
                    handle,
                    on_handle,
                }
            }
            if allow_handle_branch_drop {
                for handle in node_branch_handles(&node) {
                    BranchHandleDropTarget {
                        key: "branch-{node.id}-{handle.direction}-{handle.id}",
                        node_id: node.id.clone(),
                        handle,
                        on_drop: on_handle_branch_drop,
                    }
                }
            }
            if selected && node_deletable(&node) {
                button {
                    r#type: "button",
                    class: "absolute -right-2 -top-2 z-20 flex h-5 w-5 items-center justify-center rounded-full bg-red-500 text-[10px] font-bold text-white shadow",
                    title: "Delete selected element",
                    onclick: {
                        let delete_id = node.id.clone();
                        move |event| {
                            event.stop_propagation();
                            on_delete.call(delete_id.clone());
                        }
                    },
                    "×"
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct HandleView {
    id: String,
    kind: String,
    x: f64,
    y: f64,
}

#[derive(Clone, PartialEq)]
struct BranchHandleView {
    id: String,
    direction: String,
    x: f64,
    y: f64,
}

#[component]
fn BranchHandleDropTarget(
    node_id: String,
    handle: BranchHandleView,
    on_drop: EventHandler<(String, String, String, String)>,
) -> Element {
    let left = handle.x - 8.0;
    let top = handle.y - 8.0;
    rsx! {
        div {
            class: "absolute z-30 h-4 w-4 rounded-full border border-dashed border-[#0464fbb3] bg-[#0464fb1a] opacity-0 transition-opacity hover:bg-[#0464fb33] group-hover:opacity-100",
            style: "left:{left}px;top:{top}px;",
            title: "Drop a contact or coil to branch this BOOL pin",
            ondragover: move |event| {
                event.prevent_default();
                event.stop_propagation();
            },
            ondrop: move |event| {
                event.prevent_default();
                event.stop_propagation();
                let transfer = event.data_transfer();
                let Some(node_type) = transfer
                    .get_data("application/reactflow/ladder-blocks")
                    .filter(|value| matches!(value.as_str(), "contact" | "coil"))
                else {
                    return;
                };
                on_drop.call((
                    node_id.clone(),
                    handle.id.clone(),
                    handle.direction.clone(),
                    node_type,
                ));
            }
        }
    }
}

#[component]
fn GraphHandle(
    node_id: String,
    handle: HandleView,
    on_handle: EventHandler<(String, String, String)>,
) -> Element {
    let left = handle.x - 5.0;
    let top = handle.y - 5.0;
    let is_source = handle.kind == "source";
    rsx! {
        button {
            r#type: "button",
            class: if is_source {
                "absolute z-20 h-[10px] w-[10px] rounded-full border border-brand bg-white hover:bg-brand dark:bg-neutral-950"
            } else {
                "absolute z-20 h-[10px] w-[10px] rounded-full border border-neutral-600 bg-white hover:border-brand hover:bg-brand dark:bg-neutral-950"
            },
            style: "left:{left}px;top:{top}px;",
            title: if is_source { "Start connection" } else { "Complete connection" },
            onclick: move |event| {
                event.stop_propagation();
                on_handle.call((node_id.clone(), handle.id.clone(), handle.kind.clone()));
            }
        }
    }
}

pub(super) fn standard_function_block_pous() -> Vec<ParsedPou> {
    let Ok(unit) = iec_standard_fb_unit() else {
        return Vec::new();
    };
    unit.function_blocks
        .into_iter()
        .map(|function_block| {
            let variables = function_block
                .var_blocks
                .into_iter()
                .filter_map(|block| {
                    let class = match block.block_type {
                        CompilerVarBlockType::Input => VariableClass::Input,
                        CompilerVarBlockType::Output => VariableClass::Output,
                        CompilerVarBlockType::InOut => VariableClass::InOut,
                        _ => return None,
                    };
                    Some((class, block.declarations))
                })
                .flat_map(|(class, declarations)| {
                    declarations.into_iter().flat_map(move |declaration| {
                        let type_name = declaration.variable_type.name.clone();
                        declaration.names.into_iter().map(move |name| PlcVariable {
                            name,
                            class: Some(class),
                            variable_type: PlcVariableType::base(&type_name),
                            location: String::new(),
                            initial_value: None,
                            documentation: String::new(),
                            debug: Some(false),
                        })
                    })
                })
                .collect();
            ParsedPou {
                name: function_block.name,
                pou_type: PouType::FunctionBlock,
                interface: PouInterface {
                    return_type: None,
                    variables,
                },
                body: PouBody::text(PouLanguage::St, ""),
                documentation: "IEC standard function block".to_owned(),
            }
        })
        .collect()
}

#[component]
pub fn NodeEditor(
    node: GraphNode,
    current_pou: String,
    on_apply: EventHandler<GraphNode>,
    on_close: EventHandler<()>,
) -> Element {
    let mut store = use_context::<AppStore>();
    let initial_variable = variable_name(&node);
    let initial_variant = node
        .data
        .get("variant")
        .and_then(Value::as_str)
        .unwrap_or("default")
        .to_owned();
    let initial_content = node
        .data
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let initial_block = node
        .data
        .get("variant")
        .and_then(Value::as_object)
        .and_then(|variant| variant.get("name"))
        .and_then(Value::as_str)
        .filter(|name| *name != "???")
        .unwrap_or_default()
        .to_owned();
    let mut variable = use_signal(move || initial_variable);
    let mut variant = use_signal(move || initial_variant);
    let mut content = use_signal(move || initial_content);
    let mut block_name = use_signal(move || initial_block);
    let mut instance_name = use_signal(|| variable_name(&node));
    let project = (store.project)();
    let current_is_function = project
        .as_ref()
        .and_then(|project| {
            project
                .pous
                .iter()
                .filter_map(|file| file.parsed.as_ref())
                .find(|pou| pou.name.eq_ignore_ascii_case(&current_pou))
        })
        .is_some_and(|pou| pou.pou_type == PouType::Function);
    let mut block_choices = project
        .as_ref()
        .map(|project| {
            project
                .pous
                .iter()
                .filter_map(|file| file.parsed.as_ref())
                .filter(|pou| matches!(pou.pou_type, PouType::Function | PouType::FunctionBlock))
                .filter(|pou| !current_is_function || pou.pou_type == PouType::Function)
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !current_is_function {
        for standard in standard_function_block_pous() {
            if !block_choices
                .iter()
                .any(|pou| pou.name.eq_ignore_ascii_case(&standard.name))
            {
                block_choices.push(standard);
            }
        }
    }
    block_choices.sort_by(|left, right| left.name.cmp(&right.name));
    let block_choices_for_change = block_choices.clone();
    let block_choices_for_apply = block_choices.clone();
    let title = match node.node_type.as_str() {
        "contact" => "Edit Contact",
        "coil" => "Edit Coil",
        "block" => "Edit Block",
        "input-variable" => "Edit Input Variable",
        "output-variable" => "Edit Output Variable",
        "inout-variable" => "Edit InOut Variable",
        "connector" => "Edit Connector",
        "continuation" => "Edit Continuation",
        "comment" => "Edit Comment",
        _ => "Edit Element",
    };

    rsx! {
        div {
            class: "absolute inset-0 z-50 flex items-center justify-center bg-black/20 backdrop-blur-[1px]",
            onclick: move |_| on_close.call(()),
            div {
                class: "w-[360px] rounded-xl border border-neutral-200 bg-white p-4 shadow-xl dark:border-neutral-800 dark:bg-neutral-950",
                onclick: move |event| event.stop_propagation(),
                div { class: "mb-4 flex items-center justify-between",
                    h3 { class: "font-caption text-sm font-semibold text-neutral-1000 dark:text-neutral-50", "{title}" }
                    button {
                        r#type: "button",
                        class: "rounded px-2 py-1 text-neutral-500 hover:bg-neutral-100 dark:hover:bg-neutral-900",
                        onclick: move |_| on_close.call(()),
                        "×"
                    }
                }
                match node.node_type.as_str() {
                    "contact" | "coil" => rsx! {
                        div { class: "flex flex-col gap-3",
                            label { class: "font-caption text-[10px] font-medium text-neutral-600 dark:text-neutral-400",
                                "Variable / expression"
                                input {
                                    class: "mt-1 h-8 w-full rounded-md border border-neutral-200 bg-transparent px-2 font-mono text-xs outline-none focus:border-brand dark:border-neutral-800",
                                    value: "{variable}",
                                    oninput: move |event| variable.set(event.value()),
                                }
                            }
                            label { class: "font-caption text-[10px] font-medium text-neutral-600 dark:text-neutral-400",
                                "Variant"
                                select {
                                    class: "mt-1 h-8 w-full rounded-md border border-neutral-200 bg-white px-2 text-xs dark:border-neutral-800 dark:bg-neutral-950",
                                    value: "{variant}",
                                    onchange: move |event| variant.set(event.value()),
                                    option { value: "default", "Default" }
                                    option { value: "negated", "Negated" }
                                    option { value: "risingEdge", "Rising edge" }
                                    option { value: "fallingEdge", "Falling edge" }
                                    if node.node_type == "coil" {
                                        option { value: "set", "Set" }
                                        option { value: "reset", "Reset" }
                                    }
                                }
                            }
                        }
                    },
                    "block" => rsx! {
                        div { class: "flex flex-col gap-3",
                            label { class: "font-caption text-[10px] font-medium text-neutral-600 dark:text-neutral-400",
                                "POU"
                                select {
                                    class: "mt-1 h-8 w-full rounded-md border border-neutral-200 bg-white px-2 text-xs dark:border-neutral-800 dark:bg-neutral-950",
                                    value: "{block_name}",
                                    onchange: move |event| {
                                        let selected = event.value();
                                        block_name.set(selected.clone());
                                        let is_fb = block_choices_for_change.iter().any(|pou| {
                                            pou.name.eq_ignore_ascii_case(&selected)
                                                && pou.pou_type == PouType::FunctionBlock
                                        });
                                        if is_fb && instance_name().trim().is_empty() {
                                            instance_name.set(format!("{}_0", selected.to_ascii_lowercase()));
                                        }
                                    },
                                    option { value: "", "Select a function or function block…" }
                                    for choice in block_choices.iter() {
                                        option { key: "{choice.name}", value: "{choice.name}",
                                            if choice.pou_type == PouType::FunctionBlock { "FB · {choice.name}" } else { "Function · {choice.name}" }
                                        }
                                    }
                                }
                            }
                            if block_choices.iter().any(|pou| pou.name.eq_ignore_ascii_case(&block_name()) && pou.pou_type == PouType::FunctionBlock) {
                                label { class: "font-caption text-[10px] font-medium text-neutral-600 dark:text-neutral-400",
                                    "Instance variable"
                                    input {
                                        class: "mt-1 h-8 w-full rounded-md border border-neutral-200 bg-transparent px-2 font-mono text-xs outline-none focus:border-brand dark:border-neutral-800",
                                        value: "{instance_name}",
                                        oninput: move |event| instance_name.set(event.value()),
                                    }
                                }
                            }
                        }
                    },
                    "comment" => rsx! {
                        textarea {
                            class: "h-28 w-full resize-none rounded-md border border-neutral-200 bg-transparent p-2 text-xs outline-none focus:border-brand dark:border-neutral-800",
                            value: "{content}",
                            oninput: move |event| content.set(event.value()),
                        }
                    },
                    _ => rsx! {
                        label { class: "font-caption text-[10px] font-medium text-neutral-600 dark:text-neutral-400",
                            "Name / expression"
                            input {
                                class: "mt-1 h-8 w-full rounded-md border border-neutral-200 bg-transparent px-2 font-mono text-xs outline-none focus:border-brand dark:border-neutral-800",
                                value: "{variable}",
                                oninput: move |event| variable.set(event.value()),
                            }
                        }
                    },
                }
                div { class: "mt-5 flex justify-end gap-2",
                    button {
                        r#type: "button",
                        class: "h-8 rounded-md border border-neutral-200 px-3 text-xs text-neutral-600 hover:bg-neutral-100 dark:border-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-900",
                        onclick: move |_| on_close.call(()),
                        "Cancel"
                    }
                    button {
                        r#type: "button",
                        class: "h-8 rounded-md bg-brand px-4 text-xs font-medium text-white hover:bg-brand-medium-dark",
                        onclick: {
                            let current_pou = current_pou.clone();
                            move |_| {
                                let mut updated = node.clone();
                                match updated.node_type.as_str() {
                                    "contact" | "coil" => {
                                        set_node_variable(&mut updated, variable().trim());
                                        if let Some(data) = updated.data.as_object_mut() {
                                            data.insert("variant".to_owned(), Value::String(variant()));
                                        }
                                    }
                                    "comment" => {
                                        if let Some(data) = updated.data.as_object_mut() {
                                            data.insert("content".to_owned(), Value::String(content()));
                                        }
                                    }
                                    "block" => {
                                        let selected = block_name();
                                        let Some(pou) = block_choices_for_apply
                                            .iter()
                                            .find(|pou| pou.name.eq_ignore_ascii_case(&selected))
                                            .cloned()
                                        else {
                                            return;
                                        };
                                        let instance = if pou.pou_type == PouType::FunctionBlock {
                                            let value = instance_name().trim().to_owned();
                                            if value.is_empty() {
                                                return;
                                            }
                                            ensure_function_block_instance(&mut store, &current_pou, &value, &pou.name);
                                            value
                                        } else {
                                            String::new()
                                        };
                                        apply_block_variant(&mut updated, &pou, &instance);
                                    }
                                    _ => set_node_variable(&mut updated, variable().trim()),
                                }
                                on_apply.call(updated);
                                on_close.call(());
                            }
                        },
                        "Apply"
                    }
                }
            }
        }
    }
}

fn set_node_variable(node: &mut GraphNode, value: &str) {
    if let Some(data) = node.data.as_object_mut() {
        data.insert("variable".to_owned(), json!({ "name": value }));
    }
}

fn ensure_function_block_instance(
    store: &mut AppStore,
    current_pou: &str,
    instance_name: &str,
    function_block: &str,
) {
    store.project.with_mut(|project| {
        let Some(project) = project else {
            return;
        };
        let Some(parsed) = project
            .pous
            .iter_mut()
            .find(|file| file.name.eq_ignore_ascii_case(current_pou))
            .and_then(|file| file.parsed.as_mut())
        else {
            return;
        };
        if parsed
            .interface
            .variables
            .iter()
            .any(|variable| variable.name.eq_ignore_ascii_case(instance_name))
        {
            return;
        }
        parsed.interface.variables.push(PlcVariable {
            name: instance_name.to_owned(),
            class: Some(VariableClass::Local),
            variable_type: PlcVariableType::derived(function_block),
            location: String::new(),
            initial_value: None,
            documentation: String::new(),
            debug: Some(false),
        });
    });
}

pub(super) fn apply_block_variant(
    node: &mut GraphNode,
    pou: &crate::plc_model::ParsedPou,
    instance: &str,
) {
    let width = 216.0;
    let mut variables = pou
        .interface
        .variables
        .iter()
        .filter_map(|variable| {
            let class = match variable.class.unwrap_or(VariableClass::Local) {
                VariableClass::Input => "input",
                VariableClass::Output => "output",
                VariableClass::InOut => "inOut",
                _ => return None,
            };
            Some(json!({
                "name": variable.name,
                "class": class,
                "type": variable.variable_type,
            }))
        })
        .collect::<Vec<_>>();
    if pou.pou_type == PouType::Function {
        variables.insert(
            0,
            json!({
                "name": "OUT",
                "class": "output",
                "type": {
                    "definition": "base-type",
                    "value": pou.interface.return_type.clone().unwrap_or_else(|| "BOOL".to_owned())
                }
            }),
        );
    }
    let first_input_is_bool = variables
        .iter()
        .find(|variable| {
            matches!(
                variable.get("class").and_then(Value::as_str),
                Some("input" | "inOut")
            )
        })
        .is_some_and(variable_json_type_is_bool);
    let first_output_is_bool = variables
        .iter()
        .find(|variable| {
            matches!(
                variable.get("class").and_then(Value::as_str),
                Some("output" | "inOut")
            )
        })
        .is_some_and(variable_json_type_is_bool);
    let must_have_execution_control = !first_input_is_bool || !first_output_is_bool;
    if must_have_execution_control {
        variables.retain(|variable| {
            !matches!(
                variable.get("name").and_then(Value::as_str),
                Some("EN" | "ENO")
            )
        });
        variables.insert(
            0,
            json!({
                "name": "ENO",
                "class": "output",
                "type": { "definition": "generic-type", "value": "BOOL" }
            }),
        );
        variables.insert(
            0,
            json!({
                "name": "EN",
                "class": "input",
                "type": { "definition": "generic-type", "value": "BOOL" }
            }),
        );
    }
    let inputs = variables
        .iter()
        .filter(|variable| {
            matches!(
                variable.get("class").and_then(Value::as_str),
                Some("input" | "inOut")
            )
        })
        .collect::<Vec<_>>();
    let outputs = variables
        .iter()
        .filter(|variable| {
            matches!(
                variable.get("class").and_then(Value::as_str),
                Some("output" | "inOut")
            )
        })
        .collect::<Vec<_>>();
    let row_count = inputs.len().max(outputs.len()).max(2);
    let height = (56.0 + row_count as f64 * 40.0).max(128.0);
    let connectable = node
        .data
        .get("handles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|handle| handle.get("isConnectable").and_then(Value::as_bool) != Some(false));
    let mut input_handles = Vec::new();
    let mut output_handles = Vec::new();
    for (index, variable) in inputs.iter().enumerate() {
        let name = variable.get("name").and_then(Value::as_str).unwrap_or("IN");
        input_handles.push(editor_handle(
            name,
            "left",
            "target",
            0.0,
            36.0 + index as f64 * 40.0,
            connectable,
        ));
    }
    for (index, variable) in outputs.iter().enumerate() {
        let name = variable
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("OUT");
        output_handles.push(editor_handle(
            name,
            "right",
            "source",
            width,
            36.0 + index as f64 * 40.0,
            connectable,
        ));
    }
    let mut handles = input_handles.clone();
    handles.extend(output_handles.clone());
    node.width = Some(width);
    node.height = Some(height);
    node.measured = Some(GraphMeasured { width, height });
    if let Some(data) = node.data.as_object_mut() {
        data.insert("handles".to_owned(), Value::Array(handles));
        data.insert(
            "inputHandles".to_owned(),
            Value::Array(input_handles.clone()),
        );
        data.insert(
            "outputHandles".to_owned(),
            Value::Array(output_handles.clone()),
        );
        data.insert(
            "inputConnector".to_owned(),
            input_handles.first().cloned().unwrap_or(Value::Null),
        );
        data.insert(
            "outputConnector".to_owned(),
            output_handles.first().cloned().unwrap_or(Value::Null),
        );
        data.insert(
            "variant".to_owned(),
            json!({
                "name": pou.name,
                "type": if pou.pou_type == PouType::FunctionBlock { "function-block" } else { "function" },
                "variables": variables,
                "documentation": pou.documentation,
                "extensible": false,
            }),
        );
        let execution_control = data
            .get("executionControl")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || must_have_execution_control;
        data.insert(
            "executionControl".to_owned(),
            Value::Bool(execution_control),
        );
        data.insert(
            "lockExecutionControl".to_owned(),
            Value::Bool(must_have_execution_control),
        );
        data.insert("variable".to_owned(), json!({ "name": instance }));
    }
}

fn variable_json_type_is_bool(variable: &Value) -> bool {
    variable
        .get("type")
        .and_then(Value::as_object)
        .and_then(|variable_type| variable_type.get("value"))
        .and_then(Value::as_str)
        .is_some_and(|value| value.eq_ignore_ascii_case("BOOL"))
}

fn editor_handle(
    id: &str,
    position: &str,
    kind: &str,
    rel_x: f64,
    rel_y: f64,
    connectable: bool,
) -> Value {
    json!({
        "id": id,
        "position": position,
        "type": kind,
        "isConnectable": connectable,
        "relPosition": { "x": rel_x, "y": rel_y },
        "glbPosition": { "x": rel_x, "y": rel_y }
    })
}

#[component]
fn NodeVisual(node: GraphNode) -> Element {
    match node.node_type.as_str() {
        "contact" => rsx! { ContactVisual { node } },
        "coil" => rsx! { CoilVisual { node } },
        "powerRail" => rsx! { PowerRailVisual { node } },
        "parallel" => rsx! { ParallelVisual { node } },
        "block" => rsx! { BlockVisual { node } },
        "input-variable" | "output-variable" | "inout-variable" | "variable" => {
            rsx! { VariableVisual { node } }
        }
        "comment" => rsx! { CommentVisual { node } },
        "connector" | "continuation" => rsx! { ConnectionVisual { node } },
        _ => rsx! {
            div {
                class: "flex h-full w-full items-center justify-center rounded-md border border-neutral-850 bg-white px-2 text-xs text-neutral-1000 dark:bg-neutral-900 dark:text-neutral-50",
                "{node.node_type}"
            }
        },
    }
}

#[component]
fn ContactVisual(node: GraphNode) -> Element {
    let variable = variable_name(&node);
    let variant = node
        .data
        .get("variant")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("default");
    rsx! {
        div { class: "flex h-full w-full flex-col items-center justify-center",
            span { class: "mb-1 max-w-24 truncate text-center text-[10px] text-neutral-850 dark:text-neutral-200", "{variable}" }
            svg { width: "24", height: "24", view_box: "0 0 28 28", fill: "none", xmlns: "http://www.w3.org/2000/svg",
                line { x1: "26.75", x2: "26.75", y2: "28", stroke: "currentColor", class: "stroke-neutral-1000 dark:stroke-neutral-100", stroke_width: "1.5" }
                line { x1: "0.75", x2: "0.75", y2: "28", stroke: "currentColor", class: "stroke-neutral-1000 dark:stroke-neutral-100", stroke_width: "1.5" }
                if variant == "negated" {
                    line { y1: "-0.75", x2: "26", y2: "-0.75", transform: "matrix(0.707107 -0.707107 0.763407 0.645917 5 23.3848)", stroke: "#0464FB", stroke_width: "1.5" }
                }
                if variant == "risingEdge" {
                    text { x: "10", y: "18", fill: "#0464FB", font_size: "12", "P" }
                }
                if variant == "fallingEdge" {
                    text { x: "9", y: "18", fill: "#0464FB", font_size: "12", "N" }
                }
            }
        }
    }
}

#[component]
fn CoilVisual(node: GraphNode) -> Element {
    let variable = variable_name(&node);
    let variant = node
        .data
        .get("variant")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("default");
    let marker = match variant {
        "negated" => "/",
        "risingEdge" => "P",
        "fallingEdge" => "N",
        "set" => "S",
        "reset" => "R",
        _ => "",
    };
    rsx! {
        div { class: "flex h-full w-full flex-col items-center justify-center",
            span { class: "mb-1 max-w-24 truncate text-center text-[10px] text-neutral-850 dark:text-neutral-200", "{variable}" }
            div { class: "relative flex h-6 w-7 items-center justify-center text-neutral-1000 dark:text-neutral-100",
                span { class: "absolute left-0 text-xl leading-6", "(" }
                span { class: "text-[10px] font-semibold text-brand", "{marker}" }
                span { class: "absolute right-0 text-xl leading-6", ")" }
            }
        }
    }
}

#[component]
fn PowerRailVisual(node: GraphNode) -> Element {
    rsx! {
        div {
            class: "h-full w-full rounded-sm bg-neutral-1000 dark:bg-neutral-100",
            title: "{node.id}"
        }
    }
}

#[component]
fn ParallelVisual(node: GraphNode) -> Element {
    let side = node
        .data
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("open");
    rsx! {
        div {
            class: "h-full w-full rounded-full bg-brand",
            title: if side == "open" { "Parallel branch open" } else { "Parallel branch close" },
        }
    }
}

#[component]
fn BlockVisual(node: GraphNode) -> Element {
    let variant = node.data.get("variant").cloned().unwrap_or_default();
    let block_name = variant
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("???");
    let variables = variant
        .get("variables")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let inputs = variables
        .iter()
        .filter(|value| {
            matches!(
                value.get("class").and_then(serde_json::Value::as_str),
                Some("input" | "inOut")
            )
        })
        .filter_map(|value| value.get("name").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let outputs = variables
        .iter()
        .filter(|value| {
            matches!(
                value.get("class").and_then(serde_json::Value::as_str),
                Some("output" | "inOut")
            )
        })
        .filter_map(|value| value.get("name").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    rsx! {
        div {
            class: "relative flex h-full w-full flex-col rounded-md border border-neutral-850 bg-white text-neutral-1000 hover:border-transparent hover:ring-2 hover:ring-brand dark:bg-neutral-900 dark:text-neutral-50",
            div { class: "absolute top-2 w-full truncate bg-transparent px-1 text-center text-xs", "{block_name}" }
            for (index, connector) in inputs.iter().enumerate() {
                div { key: "i-{index}", class: "absolute left-[6px] text-xs", style: format!("top:{}px", 38 + index * 48), "{connector}" }
            }
            for (index, connector) in outputs.iter().enumerate() {
                div { key: "o-{index}", class: "absolute right-[6px] text-xs", style: format!("top:{}px", 38 + index * 48), "{connector}" }
            }
        }
    }
}

#[component]
fn VariableVisual(node: GraphNode) -> Element {
    let variable = variable_name(&node);
    rsx! {
        div {
            class: "relative flex h-full w-full items-center justify-center rounded-md border border-neutral-850 bg-white p-1 text-neutral-1000 hover:border-transparent hover:ring-2 hover:ring-brand dark:bg-neutral-900 dark:text-neutral-50",
            span { class: "w-full truncate text-center text-xs leading-3", if variable.is_empty() { "..." } else { "{variable}" } }
        }
    }
}

#[component]
fn CommentVisual(node: GraphNode) -> Element {
    let content = node
        .data
        .get("content")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    rsx! {
        div {
            class: "relative flex h-full w-full items-center justify-center rounded-md border border-neutral-850 bg-white p-1 text-neutral-1000 hover:border-transparent hover:ring-2 hover:ring-brand dark:bg-neutral-900 dark:text-neutral-50",
            div { class: "flex h-full w-full items-center justify-center p-2",
                span { class: "whitespace-pre-wrap text-center text-xs leading-3 opacity-60", if content.is_empty() { "Add some text..." } else { "{content}" } }
            }
        }
    }
}

#[component]
fn ConnectionVisual(node: GraphNode) -> Element {
    let variant = node
        .data
        .get("variant")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(node.node_type.as_str());
    let name = variable_name(&node);
    rsx! {
        div { class: "relative flex h-full w-full items-center justify-center text-neutral-1000 dark:text-neutral-50",
            if variant == "continuation" {
                div { class: "absolute left-0 h-0 w-0 border-y-[16px] border-l-[24px] border-y-transparent border-l-neutral-850 dark:border-l-neutral-100" }
            } else {
                div { class: "absolute right-0 h-0 w-0 border-y-[16px] border-r-[24px] border-y-transparent border-r-neutral-850 dark:border-r-neutral-100" }
            }
            span { class: "z-10 w-[88px] truncate text-center text-xs", if name.is_empty() { "..." } else { "{name}" } }
        }
    }
}

pub fn node_width(node: &GraphNode) -> f64 {
    node.width.unwrap_or(match node.node_type.as_str() {
        "contact" => 24.0,
        "coil" => 28.0,
        "powerRail" => 3.0,
        "block" => 216.0,
        "input-variable" | "output-variable" | "inout-variable" => 128.0,
        "variable" => 80.0,
        "comment" => 144.0,
        "connector" | "continuation" => 112.0,
        _ => 80.0,
    })
}

pub fn node_height(node: &GraphNode) -> f64 {
    node.height.unwrap_or(match node.node_type.as_str() {
        "contact" | "coil" => 24.0,
        "powerRail" => 40.0,
        "block" => 128.0,
        "input-variable" | "output-variable" | "inout-variable" => 32.0,
        "variable" => 32.0,
        "comment" => 80.0,
        "connector" | "continuation" => 32.0,
        _ => 40.0,
    })
}

fn handle_point(node: &GraphNode, handle_id: &str, source: bool) -> (f64, f64) {
    let key = if source {
        "outputHandles"
    } else {
        "inputHandles"
    };
    node.data
        .get(key)
        .and_then(Value::as_array)
        .and_then(|handles| {
            handles.iter().find(|handle| {
                handle
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| id == handle_id)
            })
        })
        .and_then(|handle| handle.get("relPosition"))
        .and_then(Value::as_object)
        .map(|position| {
            (
                node.position.x + position.get("x").and_then(Value::as_f64).unwrap_or(0.0),
                node.position.y + position.get("y").and_then(Value::as_f64).unwrap_or(0.0),
            )
        })
        .unwrap_or_else(|| node_center(node))
}

fn node_handles(node: &GraphNode) -> Vec<HandleView> {
    node.data
        .get("handles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|handle| {
            if handle.get("isConnectable").and_then(Value::as_bool) == Some(false) {
                return None;
            }
            let id = handle.get("id")?.as_str()?.to_owned();
            let kind = handle.get("type")?.as_str()?.to_owned();
            let rel = handle.get("relPosition")?.as_object()?;
            Some(HandleView {
                id,
                kind,
                x: rel.get("x").and_then(Value::as_f64).unwrap_or(0.0),
                y: rel.get("y").and_then(Value::as_f64).unwrap_or(0.0),
            })
        })
        .collect()
}

fn node_branch_handles(node: &GraphNode) -> Vec<BranchHandleView> {
    if node.node_type != "block" {
        return Vec::new();
    }
    let variables = node
        .data
        .get("variant")
        .and_then(|variant| variant.get("variables"))
        .and_then(Value::as_array);
    let Some(variables) = variables else {
        return Vec::new();
    };
    let is_bool_handle = |handle_id: &str| {
        variables.iter().any(|variable| {
            variable.get("name").and_then(Value::as_str) == Some(handle_id)
                && variable_json_type_is_bool(variable)
        })
    };
    let mut output = Vec::new();
    for (direction, key) in [("input", "inputHandles"), ("output", "outputHandles")] {
        let Some(handles) = node.data.get(key).and_then(Value::as_array) else {
            continue;
        };
        for handle in handles.iter().skip(1) {
            let Some(id) = handle.get("id").and_then(Value::as_str) else {
                continue;
            };
            if !is_bool_handle(id) {
                continue;
            }
            let Some(rel) = handle.get("relPosition").and_then(Value::as_object) else {
                continue;
            };
            output.push(BranchHandleView {
                id: id.to_owned(),
                direction: direction.to_owned(),
                x: rel.get("x").and_then(Value::as_f64).unwrap_or(0.0),
                y: rel.get("y").and_then(Value::as_f64).unwrap_or(0.0),
            });
        }
    }
    output
}

fn node_deletable(node: &GraphNode) -> bool {
    node.data
        .get("deletable")
        .and_then(Value::as_bool)
        .unwrap_or(node.node_type != "powerRail")
}

fn node_center(node: &GraphNode) -> (f64, f64) {
    (
        node.position.x + node_width(node) / 2.0,
        node.position.y + node_height(node) / 2.0,
    )
}

fn variable_name(node: &GraphNode) -> String {
    node.data
        .get("variable")
        .and_then(|value| value.get("name"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn flow_point(x: f64, y: f64, viewport: GraphViewport) -> (f64, f64) {
    (
        (x - viewport.x) / viewport.zoom.max(0.000_001),
        (y - viewport.y) / viewport.zoom.max(0.000_001),
    )
}

fn zoom_about(viewport: GraphViewport, factor: f64, screen_x: f64, screen_y: f64) -> GraphViewport {
    let next_zoom = (viewport.zoom * factor).clamp(0.25, 2.5);
    let (flow_x, flow_y) = flow_point(screen_x, screen_y, viewport);
    GraphViewport {
        x: screen_x - flow_x * next_zoom,
        y: screen_y - flow_y * next_zoom,
        zoom: next_zoom,
    }
}

fn snap_grid(value: f64) -> f64 {
    (value / 16.0).round() * 16.0
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{apply_block_variant, flow_point, standard_function_block_pous, zoom_about};
    use crate::plc_model::{
        GraphNode, GraphPosition, GraphViewport, ParsedPou, PlcVariable, PlcVariableType, PouBody,
        PouInterface, PouLanguage, PouType, VariableClass,
    };

    fn block(connectable: bool) -> GraphNode {
        GraphNode {
            id: "block-1".to_owned(),
            node_type: "block".to_owned(),
            position: GraphPosition { x: 0.0, y: 0.0 },
            height: Some(128.0),
            width: Some(216.0),
            measured: None,
            draggable: true,
            selectable: true,
            data: json!({
                "handles": [{"id":"old","type":"source","isConnectable":connectable}],
                "inputHandles": [],
                "outputHandles": [],
                "inputConnector": null,
                "outputConnector": null,
                "variant": {"name":"???","type":"generic","variables":[]},
                "variable": {"name":""},
                "deletable": true
            }),
        }
    }

    fn variable(name: &str, class: VariableClass, type_name: &str) -> PlcVariable {
        PlcVariable {
            name: name.to_owned(),
            class: Some(class),
            variable_type: PlcVariableType::base(type_name),
            location: String::new(),
            initial_value: None,
            documentation: String::new(),
            debug: Some(false),
        }
    }

    fn pou(name: &str, pou_type: PouType, return_type: Option<&str>) -> ParsedPou {
        ParsedPou {
            name: name.to_owned(),
            pou_type,
            interface: PouInterface {
                return_type: return_type.map(str::to_owned),
                variables: vec![
                    variable("IN", VariableClass::Input, "BOOL"),
                    variable("Q", VariableClass::Output, "BOOL"),
                ],
            },
            body: PouBody::text(PouLanguage::St, ""),
            documentation: "test POU".to_owned(),
        }
    }

    #[test]
    fn function_block_variant_builds_compiler_facing_signature_and_instance() {
        let mut node = block(true);
        let pou = pou("Motor", PouType::FunctionBlock, None);
        apply_block_variant(&mut node, &pou, "motor0");

        let variant = node.data["variant"].as_object().unwrap();
        assert_eq!(variant["name"], "Motor");
        assert_eq!(variant["type"], "function-block");
        assert_eq!(node.data["variable"]["name"], "motor0");
        assert_eq!(node.data["inputHandles"][0]["id"], "IN");
        assert_eq!(node.data["outputHandles"][0]["id"], "Q");
        assert_eq!(node.data["inputHandles"][0]["isConnectable"], true);
        assert_eq!(node.width, Some(216.0));
        assert!(node.height.unwrap() >= 128.0);
    }

    #[test]
    fn function_variant_injects_out_and_preserves_nonconnectable_ladder_handles() {
        let mut node = block(false);
        let mut pou = pou("Choose", PouType::Function, Some("INT"));
        pou.interface
            .variables
            .retain(|variable| variable.class == Some(VariableClass::Input));
        apply_block_variant(&mut node, &pou, "");

        assert_eq!(node.data["variant"]["type"], "function");
        assert_eq!(node.data["variant"]["variables"][0]["name"], "EN");
        assert_eq!(node.data["variant"]["variables"][1]["name"], "ENO");
        assert!(node.data["variant"]["variables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|variable| { variable["name"] == "OUT" && variable["type"]["value"] == "INT" }));
        assert_eq!(node.data["variable"]["name"], "");
        assert_eq!(node.data["executionControl"], true);
        assert_eq!(node.data["lockExecutionControl"], true);
        assert_eq!(node.data["inputHandles"][0]["id"], "EN");
        assert_eq!(node.data["inputHandles"][0]["isConnectable"], false);
        assert_eq!(node.data["outputHandles"][0]["id"], "ENO");
        assert_eq!(node.data["outputHandles"][0]["isConnectable"], false);
        assert_eq!(node.data["outputHandles"][1]["id"], "OUT");
    }

    #[test]
    fn standard_fb_picker_uses_the_owned_iec_source_signature() {
        let ton = standard_function_block_pous()
            .into_iter()
            .find(|pou| pou.name.eq_ignore_ascii_case("TON"))
            .expect("bundled TON function block");
        let names = ton
            .interface
            .variables
            .iter()
            .map(|variable| (variable.name.as_str(), variable.class))
            .collect::<Vec<_>>();
        assert!(names.contains(&("IN", Some(VariableClass::Input))));
        assert!(names.contains(&("PT", Some(VariableClass::Input))));
        assert!(names.contains(&("Q", Some(VariableClass::Output))));
        assert!(names.contains(&("ET", Some(VariableClass::Output))));

        let mut node = block(true);
        apply_block_variant(&mut node, &ton, "timer0");
        assert_eq!(node.data["variant"]["name"], "TON");
        assert_eq!(node.data["variant"]["type"], "function-block");
        assert_eq!(node.data["variable"]["name"], "timer0");
        assert!(node.data["inputHandles"]
            .as_array()
            .unwrap()
            .iter()
            .any(|handle| handle["id"] == "PT"));
        assert!(node.data["outputHandles"]
            .as_array()
            .unwrap()
            .iter()
            .any(|handle| handle["id"] == "ET"));
    }

    #[test]
    fn zoom_preserves_the_flow_point_under_the_cursor() {
        let viewport = GraphViewport {
            x: 40.0,
            y: -20.0,
            zoom: 1.25,
        };
        let anchor = (360.0, 180.0);
        let before = flow_point(anchor.0, anchor.1, viewport);
        let zoomed = zoom_about(viewport, 1.1, anchor.0, anchor.1);
        let after = flow_point(anchor.0, anchor.1, zoomed);
        assert!((before.0 - after.0).abs() < 1e-9);
        assert!((before.1 - after.1).abs() < 1e-9);
        assert!(zoomed.zoom > viewport.zoom);

        let clamped_low = zoom_about(GraphViewport::default(), 0.0001, 0.0, 0.0);
        let clamped_high = zoom_about(GraphViewport::default(), 100.0, 0.0, 0.0);
        assert_eq!(clamped_low.zoom, 0.25);
        assert_eq!(clamped_high.zoom, 2.5);
    }
}
