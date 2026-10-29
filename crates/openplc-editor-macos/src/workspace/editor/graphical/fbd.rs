use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::app::{AppStore, ConsoleEntry};
use crate::native_clipboard;
use crate::plc_model::{
    FbdFlow, FbdRung, GraphEdge, GraphMeasured, GraphNode, GraphPosition, GraphViewport,
};
use crate::workspace::editor::graphical::node::{GraphCanvas, NodeEditor};

const FBD_CLIPBOARD_KIND: &str = "openplc-fbd-selection";
const FBD_CLIPBOARD_VERSION: u8 = 1;
const FBD_CLIPBOARD_MAX_BYTES: usize = 1_048_576;
const FBD_CLIPBOARD_MAX_NODES: usize = 512;
const FBD_CLIPBOARD_MAX_EDGES: usize = 2048;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FbdClipboardPayload {
    kind: String,
    version: u8,
    nodes: Vec<GraphNode>,
    edges: Vec<GraphEdge>,
}

#[component]
pub fn FbdEditor(pou_name: String) -> Element {
    let mut store = use_context::<AppStore>();
    let mut editing_node = use_signal(|| None::<String>);
    let flow = parse_flow(&store.current_content(), &pou_name);
    let Ok(flow) = flow else {
        return rsx! {
            div { class: "h-full w-full",
                p { class: "p-4 font-mono text-xs text-red-500", "Invalid Functional Block Diagram body" }
            }
        };
    };
    let width = flow
        .rung
        .nodes
        .iter()
        .map(|node| node.position.x + node.width.unwrap_or(216.0))
        .fold(900.0_f64, f64::max)
        + 80.0;
    let height = flow
        .rung
        .nodes
        .iter()
        .map(|node| node.position.y + node.height.unwrap_or(128.0))
        .fold(520.0_f64, f64::max)
        + 80.0;

    rsx! {
        div { class: "h-full w-full rounded-lg border p-1 dark:border-neutral-800",
            GraphCanvas {
                key: "fbd-{pou_name}",
                nodes: flow.rung.nodes.clone(),
                edges: flow.rung.edges.clone(),
                width,
                height,
                viewport: flow.viewport,
                allow_pan_zoom: true,
                on_viewport: {
                    let pou_name = pou_name.clone();
                    move |viewport: GraphViewport| {
                        if let Ok(mut flow) = parse_flow(&store.current_content(), &pou_name) {
                            if flow.viewport != viewport {
                                flow.viewport = viewport;
                                write_flow(&mut store, &flow);
                            }
                        }
                    }
                },
                on_move: {
                    let pou_name = pou_name.clone();
                    move |(node_id, x, y)| {
                        if let Ok(mut flow) = parse_flow(&store.current_content(), &pou_name) {
                            if let Some(node) = flow.rung.nodes.iter_mut().find(|node| node.id == node_id) {
                                if node.draggable {
                                    node.position = GraphPosition { x: snap_grid(x), y: snap_grid(y) };
                                    refresh_handle_positions(node);
                                    write_flow(&mut store, &flow);
                                }
                            }
                        }
                    }
                },
                on_add: {
                    let pou_name = pou_name.clone();
                    move |(node_type, x, y): (String, f64, f64)| {
                        if let Ok(mut flow) = parse_flow(&store.current_content(), &pou_name) {
                            if let Some(node) = new_fbd_node(&node_type, x, y) {
                                flow.rung.nodes.push(node);
                                write_flow(&mut store, &flow);
                            }
                        }
                    }
                },
                on_delete: {
                    let pou_name = pou_name.clone();
                    move |node_id: String| {
                        if let Ok(mut flow) = parse_flow(&store.current_content(), &pou_name) {
                            flow.rung.nodes.retain(|node| node.id != node_id);
                            flow.rung.edges.retain(|edge| edge.source != node_id && edge.target != node_id);
                            write_flow(&mut store, &flow);
                        }
                    }
                },
                on_delete_edge: {
                    let pou_name = pou_name.clone();
                    move |edge_id: String| {
                        if let Ok(mut flow) = parse_flow(&store.current_content(), &pou_name) {
                            let before = flow.rung.edges.len();
                            flow.rung.edges.retain(|edge| edge.id != edge_id);
                            if flow.rung.edges.len() != before {
                                write_flow(&mut store, &flow);
                            }
                        }
                    }
                },
                on_copy_selection: {
                    let pou_name = pou_name.clone();
                    move |selected: Vec<String>| {
                        let result = parse_flow(&store.current_content(), &pou_name)
                            .map_err(|error| error.to_string())
                            .and_then(|flow| copy_fbd_selection(&flow, &selected))
                            .and_then(|text| native_clipboard::write_text(&text));
                        if let Err(error) = result {
                            push_clipboard_error(&mut store, error);
                        }
                    }
                },
                on_paste: {
                    let pou_name = pou_name.clone();
                    move |_| {
                        let result = native_clipboard::read_text().and_then(|text| {
                            let mut flow = parse_flow(&store.current_content(), &pou_name)
                                .map_err(|error| error.to_string())?;
                            if paste_fbd_selection(&mut flow, &text)? {
                                write_flow(&mut store, &flow);
                            }
                            Ok(())
                        });
                        if let Err(error) = result {
                            push_clipboard_error(&mut store, error);
                        }
                    }
                },
                allow_edge_selection: true,
                allow_handle_branch_drop: false,
                on_handle_branch_drop: move |_| {},
                on_connect: {
                    let pou_name = pou_name.clone();
                    move |(source, source_handle, target, target_handle): (String, String, String, String)| {
                        if let Ok(mut flow) = parse_flow(&store.current_content(), &pou_name) {
                            if connect_fbd_nodes(
                                &mut flow,
                                source,
                                source_handle,
                                target,
                                target_handle,
                            ) {
                                write_flow(&mut store, &flow);
                            }
                        }
                    }
                },
                on_edit: move |node_id| editing_node.set(Some(node_id))
            }
            if let Some(node_id) = editing_node() {
                if let Some(node) = flow.rung.nodes.iter().find(|node| node.id == node_id).cloned() {
                    NodeEditor {
                        node,
                        current_pou: pou_name.clone(),
                        on_apply: {
                            let pou_name = pou_name.clone();
                            move |updated: GraphNode| {
                                if let Ok(mut next) = parse_flow(&store.current_content(), &pou_name) {
                                    if let Some(node) = next.rung.nodes.iter_mut().find(|node| node.id == updated.id) {
                                        *node = updated;
                                        refresh_handle_positions(node);
                                        write_flow(&mut store, &next);
                                    }
                                }
                            }
                        },
                        on_close: move |_| editing_node.set(None),
                    }
                }
            }
        }
    }
}

fn connect_fbd_nodes(
    flow: &mut FbdFlow,
    source: String,
    source_handle: String,
    target: String,
    target_handle: String,
) -> bool {
    if source == target {
        return false;
    }
    let duplicate = flow.rung.edges.iter().any(|edge| {
        edge.source == source
            && edge.source_handle == source_handle
            && edge.target == target
            && edge.target_handle == target_handle
    });
    if duplicate {
        return false;
    }
    flow.rung
        .edges
        .retain(|edge| !(edge.target == target && edge.target_handle == target_handle));
    flow.rung.edges.push(crate::plc_model::GraphEdge {
        id: format!("e_{source}_{target}__{source_handle}_{target_handle}"),
        source,
        source_handle,
        target,
        target_handle,
        edge_type: Some("smoothstep".to_owned()),
    });
    true
}

fn parse_flow(content: &str, pou_name: &str) -> Result<FbdFlow, serde_json::Error> {
    if content.trim().is_empty() {
        return Ok(FbdFlow {
            name: pou_name.to_owned(),
            viewport: GraphViewport::default(),
            rung: FbdRung {
                comment: String::new(),
                nodes: Vec::new(),
                edges: Vec::new(),
            },
        });
    }
    serde_json::from_str(content)
}

fn write_flow(store: &mut AppStore, flow: &FbdFlow) {
    if let Ok(content) = serde_json::to_string_pretty(flow) {
        store.set_current_content(content);
    }
}

fn push_clipboard_error(store: &mut AppStore, message: String) {
    store.console.with_mut(|console| {
        console.push(ConsoleEntry {
            level: "error",
            message: format!("FBD clipboard: {message}"),
        });
    });
}

fn copy_fbd_selection(flow: &FbdFlow, selected: &[String]) -> Result<String, String> {
    if selected.is_empty() {
        return Err("no nodes are selected".to_owned());
    }
    let selected = selected.iter().cloned().collect::<HashSet<_>>();
    let nodes = flow
        .rung
        .nodes
        .iter()
        .filter(|node| selected.contains(&node.id))
        .cloned()
        .collect::<Vec<_>>();
    if nodes.is_empty() {
        return Err("selected nodes are no longer present".to_owned());
    }
    if nodes.len() > FBD_CLIPBOARD_MAX_NODES {
        return Err(format!(
            "selection contains {} nodes; maximum is {FBD_CLIPBOARD_MAX_NODES}",
            nodes.len()
        ));
    }
    let copied_ids = nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<HashSet<_>>();
    let edges = flow
        .rung
        .edges
        .iter()
        .filter(|edge| {
            copied_ids.contains(edge.source.as_str()) && copied_ids.contains(edge.target.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    if edges.len() > FBD_CLIPBOARD_MAX_EDGES {
        return Err(format!(
            "selection contains {} internal edges; maximum is {FBD_CLIPBOARD_MAX_EDGES}",
            edges.len()
        ));
    }
    let payload = FbdClipboardPayload {
        kind: FBD_CLIPBOARD_KIND.to_owned(),
        version: FBD_CLIPBOARD_VERSION,
        nodes,
        edges,
    };
    serde_json::to_string(&payload).map_err(|error| format!("serialize selection: {error}"))
}

fn paste_fbd_selection(flow: &mut FbdFlow, text: &str) -> Result<bool, String> {
    if text.len() > FBD_CLIPBOARD_MAX_BYTES {
        return Err(format!(
            "clipboard payload is {} bytes; maximum is {FBD_CLIPBOARD_MAX_BYTES}",
            text.len()
        ));
    }
    let Ok(payload) = serde_json::from_str::<FbdClipboardPayload>(text) else {
        return Ok(false);
    };
    if payload.kind != FBD_CLIPBOARD_KIND || payload.version != FBD_CLIPBOARD_VERSION {
        return Ok(false);
    }
    if payload.nodes.is_empty() {
        return Ok(false);
    }
    if payload.nodes.len() > FBD_CLIPBOARD_MAX_NODES
        || payload.edges.len() > FBD_CLIPBOARD_MAX_EDGES
    {
        return Err("clipboard selection exceeds FBD graph limits".to_owned());
    }

    let mut old_ids = HashSet::new();
    for node in &payload.nodes {
        if node.id.is_empty() || !old_ids.insert(node.id.clone()) {
            return Err("clipboard selection contains duplicate or empty node ids".to_owned());
        }
    }
    if payload
        .edges
        .iter()
        .any(|edge| !old_ids.contains(&edge.source) || !old_ids.contains(&edge.target))
    {
        return Err("clipboard selection contains an edge outside the copied node set".to_owned());
    }

    let id_map = payload
        .nodes
        .iter()
        .map(|node| {
            (
                node.id.clone(),
                format!("{}_{}", node.node_type.to_ascii_uppercase(), Uuid::new_v4()),
            )
        })
        .collect::<HashMap<_, _>>();

    let mut nodes = payload.nodes;
    for node in &mut nodes {
        node.id = id_map
            .get(&node.id)
            .cloned()
            .ok_or_else(|| "clipboard node id remap failed".to_owned())?;
        node.position.x = snap_grid(node.position.x + 32.0);
        node.position.y = snap_grid(node.position.y + 32.0);
        if let Some(data) = node.data.as_object_mut() {
            if data.contains_key("numericId") {
                data.insert("numericId".to_owned(), Value::String(numeric_id()));
            }
        }
        refresh_handle_positions(node);
    }

    let mut edges = payload.edges;
    for edge in &mut edges {
        edge.source = id_map
            .get(&edge.source)
            .cloned()
            .ok_or_else(|| "clipboard edge source remap failed".to_owned())?;
        edge.target = id_map
            .get(&edge.target)
            .cloned()
            .ok_or_else(|| "clipboard edge target remap failed".to_owned())?;
        edge.id = format!("e_{}", Uuid::new_v4());
    }

    flow.rung.nodes.extend(nodes);
    flow.rung.edges.extend(edges);
    Ok(true)
}

fn new_fbd_node(node_type: &str, x: f64, y: f64) -> Option<GraphNode> {
    let id = format!("{}_{}", node_type.to_ascii_uppercase(), Uuid::new_v4());
    match node_type {
        "block" => Some(block_node(id, x, y)),
        "input-variable" | "output-variable" | "inout-variable" => {
            Some(variable_node(id, node_type, x, y))
        }
        "connector" | "continuation" => Some(connection_node(id, node_type, x, y)),
        "comment" => Some(comment_node(id, x, y)),
        _ => None,
    }
}

fn block_node(id: String, x: f64, y: f64) -> GraphNode {
    let variant = json!({
        "name": "???",
        "type": "generic",
        "variables": [
            { "name": "???", "class": "input", "type": { "definition": "base-type", "value": "BOOL" } },
            { "name": "???", "class": "output", "type": { "definition": "base-type", "value": "BOOL" } }
        ],
        "documentation": ""
    });
    let input = handle("???", "left", "target", x, y + 36.0, 0.0, 36.0);
    let output = handle("???", "right", "source", x + 216.0, y + 36.0, 216.0, 36.0);
    GraphNode {
        id,
        node_type: "block".to_owned(),
        position: GraphPosition { x, y },
        height: Some(128.0),
        width: Some(216.0),
        measured: Some(GraphMeasured {
            width: 216.0,
            height: 128.0,
        }),
        draggable: true,
        selectable: true,
        data: json!({
            "handles": [input.clone(), output.clone()],
            "inputHandles": [input.clone()],
            "outputHandles": [output.clone()],
            "inputConnector": input,
            "outputConnector": output,
            "numericId": numeric_id(),
            "variant": variant,
            "variable": { "name": "" },
            "executionOrder": 0,
            "executionControl": false,
            "draggable": true,
            "selectable": true,
            "deletable": true
        }),
    }
}

fn variable_node(id: String, node_type: &str, x: f64, y: f64) -> GraphNode {
    let input = matches!(node_type, "output-variable" | "inout-variable")
        .then(|| handle("input-variable", "left", "target", x, y + 16.0, 0.0, 16.0));
    let output = matches!(node_type, "input-variable" | "inout-variable").then(|| {
        handle(
            "output-variable",
            "right",
            "source",
            x + 112.0,
            y + 16.0,
            112.0,
            16.0,
        )
    });
    let handles = [input.clone(), output.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    GraphNode {
        id,
        node_type: node_type.to_owned(),
        position: GraphPosition { x, y },
        height: Some(32.0),
        width: Some(128.0),
        measured: Some(GraphMeasured {
            width: 128.0,
            height: 32.0,
        }),
        draggable: true,
        selectable: true,
        data: json!({
            "handles": handles,
            "inputHandles": input.clone().into_iter().collect::<Vec<_>>(),
            "outputHandles": output.clone().into_iter().collect::<Vec<_>>(),
            "inputConnector": input,
            "outputConnector": output,
            "numericId": numeric_id(),
            "variable": { "id": "", "name": "" },
            "executionOrder": 0,
            "variant": node_type,
            "negated": false,
            "draggable": true,
            "selectable": true,
            "deletable": true
        }),
    }
}

fn connection_node(id: String, node_type: &str, x: f64, y: f64) -> GraphNode {
    let input = (node_type == "connector")
        .then(|| handle("input", "left", "target", x, y + 16.0, 0.0, 16.0));
    let output = (node_type == "continuation").then(|| {
        handle(
            "output",
            "right",
            "source",
            x + 112.0,
            y + 16.0,
            112.0,
            16.0,
        )
    });
    let handles = [input.clone(), output.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    GraphNode {
        id,
        node_type: node_type.to_owned(),
        position: GraphPosition { x, y },
        height: Some(32.0),
        width: Some(112.0),
        measured: Some(GraphMeasured {
            width: 112.0,
            height: 32.0,
        }),
        draggable: true,
        selectable: true,
        data: json!({
            "handles": handles,
            "inputHandles": input.clone().into_iter().collect::<Vec<_>>(),
            "outputHandles": output.clone().into_iter().collect::<Vec<_>>(),
            "inputConnector": input,
            "outputConnector": output,
            "numericId": numeric_id(),
            "executionOrder": 0,
            "variant": node_type,
            "variable": { "id": "", "name": "" },
            "draggable": true,
            "selectable": true,
            "deletable": true
        }),
    }
}

fn comment_node(id: String, x: f64, y: f64) -> GraphNode {
    GraphNode {
        id,
        node_type: "comment".to_owned(),
        position: GraphPosition { x, y },
        height: Some(80.0),
        width: Some(144.0),
        measured: Some(GraphMeasured {
            width: 144.0,
            height: 80.0,
        }),
        draggable: true,
        selectable: true,
        data: json!({
            "numericId": numeric_id(),
            "draggable": true,
            "selectable": true,
            "deletable": true,
            "content": ""
        }),
    }
}

fn refresh_handle_positions(node: &mut GraphNode) {
    let x = node.position.x;
    let y = node.position.y;
    for key in ["handles", "inputHandles", "outputHandles"] {
        if let Some(handles) = node.data.get_mut(key).and_then(Value::as_array_mut) {
            for handle in handles {
                let (rel_x, rel_y) = handle
                    .get("relPosition")
                    .and_then(Value::as_object)
                    .map(|rel| {
                        (
                            rel.get("x").and_then(Value::as_f64).unwrap_or(0.0),
                            rel.get("y").and_then(Value::as_f64).unwrap_or(0.0),
                        )
                    })
                    .unwrap_or((0.0, 0.0));
                if let Some(object) = handle.as_object_mut() {
                    object.insert(
                        "glbPosition".to_owned(),
                        json!({ "x": x + rel_x, "y": y + rel_y }),
                    );
                }
            }
        }
    }
    for key in ["inputConnector", "outputConnector"] {
        if let Some(handle) = node.data.get_mut(key) {
            let (rel_x, rel_y) = handle
                .get("relPosition")
                .and_then(Value::as_object)
                .map(|rel| {
                    (
                        rel.get("x").and_then(Value::as_f64).unwrap_or(0.0),
                        rel.get("y").and_then(Value::as_f64).unwrap_or(0.0),
                    )
                })
                .unwrap_or((0.0, 0.0));
            if let Some(object) = handle.as_object_mut() {
                object.insert(
                    "glbPosition".to_owned(),
                    json!({ "x": x + rel_x, "y": y + rel_y }),
                );
            }
        }
    }
}

fn snap_grid(value: f64) -> f64 {
    (value / 16.0).round() * 16.0
}

fn handle(
    id: &str,
    position: &str,
    kind: &str,
    glb_x: f64,
    glb_y: f64,
    rel_x: f64,
    rel_y: f64,
) -> Value {
    json!({
        "id": id,
        "position": position,
        "type": kind,
        "glbPosition": { "x": glb_x, "y": glb_y },
        "relPosition": { "x": rel_x, "y": rel_y }
    })
}

fn numeric_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let suffix = millis % 1_000_000;
    let random = u128::from_le_bytes(*Uuid::new_v4().as_bytes()) % 10_000_000;
    (suffix + random).to_string()
}

#[cfg(test)]
mod tests {
    use open_rust_plc_compiler::graphical::emit_fbd_value;
    use serde_json::to_value;

    use crate::plc_model::{FbdFlow, FbdRung, GraphEdge, GraphViewport};

    use super::{
        connect_fbd_nodes, copy_fbd_selection, new_fbd_node, parse_flow, paste_fbd_selection,
        FbdClipboardPayload,
    };

    #[test]
    fn fbd_connection_replaces_only_the_target_handle_input() {
        let mut flow = FbdFlow {
            name: "main".to_owned(),
            viewport: GraphViewport::default(),
            rung: FbdRung {
                comment: String::new(),
                nodes: Vec::new(),
                edges: vec![GraphEdge {
                    id: "old".to_owned(),
                    source: "A".to_owned(),
                    source_handle: "OUT".to_owned(),
                    target: "C".to_owned(),
                    target_handle: "IN".to_owned(),
                    edge_type: Some("smoothstep".to_owned()),
                }],
            },
        };
        assert!(connect_fbd_nodes(
            &mut flow,
            "B".to_owned(),
            "OUT".to_owned(),
            "C".to_owned(),
            "IN".to_owned(),
        ));
        assert_eq!(flow.rung.edges.len(), 1);
        assert_eq!(flow.rung.edges[0].source, "B");
        assert_eq!(flow.rung.edges[0].target, "C");
        assert!(!connect_fbd_nodes(
            &mut flow,
            "B".to_owned(),
            "OUT".to_owned(),
            "C".to_owned(),
            "IN".to_owned(),
        ));
        assert_eq!(flow.rung.edges.len(), 1);
        assert!(!connect_fbd_nodes(
            &mut flow,
            "C".to_owned(),
            "OUT".to_owned(),
            "C".to_owned(),
            "IN2".to_owned(),
        ));
    }

    #[test]
    fn fbd_viewport_round_trips_in_the_saved_body() {
        let flow = parse_flow(
            r#"{"name":"main","viewport":{"x":12.5,"y":-7.0,"zoom":1.5},"rung":{"comment":"","nodes":[],"edges":[]}}"#,
            "main",
        )
        .unwrap();
        assert_eq!(
            flow.viewport,
            GraphViewport {
                x: 12.5,
                y: -7.0,
                zoom: 1.5,
            }
        );
        let saved = serde_json::to_value(&flow).unwrap();
        assert_eq!(saved["viewport"]["x"], 12.5);
        assert_eq!(saved["viewport"]["y"], -7.0);
        assert_eq!(saved["viewport"]["zoom"], 1.5);

        let legacy = parse_flow(
            r#"{"name":"main","rung":{"comment":"","nodes":[],"edges":[]}}"#,
            "main",
        )
        .unwrap();
        assert_eq!(legacy.viewport, GraphViewport::default());
    }

    #[test]
    fn edited_fbd_variables_compile_through_the_owned_graphical_walker() {
        let mut input = new_fbd_node("input-variable", 32.0, 32.0).unwrap();
        input.data["variable"]["name"] = serde_json::Value::String("source".to_owned());
        let input_id = input.id.clone();
        let mut output = new_fbd_node("output-variable", 240.0, 32.0).unwrap();
        output.data["variable"]["name"] = serde_json::Value::String("sink".to_owned());
        let output_id = output.id.clone();
        let mut flow = FbdFlow {
            name: "main".to_owned(),
            viewport: GraphViewport::default(),
            rung: FbdRung {
                comment: String::new(),
                nodes: vec![input, output],
                edges: Vec::new(),
            },
        };
        assert!(connect_fbd_nodes(
            &mut flow,
            input_id,
            "output-variable".to_owned(),
            output_id,
            "input-variable".to_owned(),
        ));
        let emitted = emit_fbd_value(&to_value(flow).unwrap()).unwrap();
        assert!(emitted.warnings.is_empty(), "{:?}", emitted.warnings);
        assert!(emitted.body_st.contains("sink"));
        assert!(emitted.body_st.contains("source"));
    }

    #[test]
    fn fbd_clipboard_round_trip_remaps_ids_edges_and_positions() {
        let input = new_fbd_node("input-variable", 32.0, 32.0).unwrap();
        let input_id = input.id.clone();
        let input_numeric_id = input.data["numericId"].as_str().unwrap().to_owned();
        let output = new_fbd_node("output-variable", 240.0, 32.0).unwrap();
        let output_id = output.id.clone();
        let mut flow = FbdFlow {
            name: "main".to_owned(),
            viewport: GraphViewport::default(),
            rung: FbdRung {
                comment: String::new(),
                nodes: vec![input, output],
                edges: Vec::new(),
            },
        };
        assert!(connect_fbd_nodes(
            &mut flow,
            input_id.clone(),
            "output-variable".to_owned(),
            output_id.clone(),
            "input-variable".to_owned(),
        ));

        let text = copy_fbd_selection(&flow, &[input_id.clone(), output_id.clone()]).unwrap();
        let payload: FbdClipboardPayload = serde_json::from_str(&text).unwrap();
        assert_eq!(payload.nodes.len(), 2);
        assert_eq!(payload.edges.len(), 1);
        assert!(paste_fbd_selection(&mut flow, &text).unwrap());

        assert_eq!(flow.rung.nodes.len(), 4);
        assert_eq!(flow.rung.edges.len(), 2);
        let pasted = &flow.rung.nodes[2..];
        let pasted_ids = pasted
            .iter()
            .map(|node| node.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        assert!(!pasted_ids.contains(input_id.as_str()));
        assert!(!pasted_ids.contains(output_id.as_str()));
        assert_eq!(pasted[0].position.x, 64.0);
        assert_eq!(pasted[0].position.y, 64.0);
        assert_eq!(pasted[1].position.x, 272.0);
        assert_eq!(pasted[1].position.y, 64.0);
        assert_ne!(
            pasted[0].data["numericId"].as_str().unwrap(),
            input_numeric_id
        );
        assert_eq!(pasted[0].data["outputConnector"]["glbPosition"]["x"], 176.0);
        assert_eq!(pasted[0].data["outputConnector"]["glbPosition"]["y"], 80.0);

        let edge = flow.rung.edges.last().unwrap();
        assert!(pasted_ids.contains(edge.source.as_str()));
        assert!(pasted_ids.contains(edge.target.as_str()));
        assert_eq!(edge.source_handle, "output-variable");
        assert_eq!(edge.target_handle, "input-variable");
    }

    #[test]
    fn fbd_clipboard_ignores_unrelated_text_and_rejects_external_edges() {
        let mut flow = FbdFlow {
            name: "main".to_owned(),
            viewport: GraphViewport::default(),
            rung: FbdRung {
                comment: String::new(),
                nodes: Vec::new(),
                edges: Vec::new(),
            },
        };
        assert!(!paste_fbd_selection(&mut flow, "plain text").unwrap());
        assert!(flow.rung.nodes.is_empty());

        let node = new_fbd_node("comment", 0.0, 0.0).unwrap();
        let payload = FbdClipboardPayload {
            kind: super::FBD_CLIPBOARD_KIND.to_owned(),
            version: super::FBD_CLIPBOARD_VERSION,
            nodes: vec![node.clone()],
            edges: vec![GraphEdge {
                id: "bad".to_owned(),
                source: node.id,
                source_handle: "out".to_owned(),
                target: "outside".to_owned(),
                target_handle: "in".to_owned(),
                edge_type: None,
            }],
        };
        let text = serde_json::to_string(&payload).unwrap();
        let error = paste_fbd_selection(&mut flow, &text).unwrap_err();
        assert!(error.contains("outside the copied node set"));
        assert!(flow.rung.nodes.is_empty());
    }
}
