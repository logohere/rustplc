use std::collections::{HashMap, HashSet};

use dioxus::prelude::*;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::app::AppStore;
use crate::plc_model::{
    GraphEdge, GraphMeasured, GraphNode, GraphPosition, GraphViewport, LadderFlow, LadderRung,
};
use crate::workspace::editor::graphical::node::{GraphCanvas, NodeEditor};
use crate::workspace_icons::{DragHandleIcon, DuplicateIcon};

const DEFAULT_BOUNDS: [f64; 2] = [300.0, 100.0];

#[component]
pub fn LadderEditor(pou_name: String) -> Element {
    let mut store = use_context::<AppStore>();
    let flow = parse_flow(&store.current_content(), &pou_name);
    let Ok(flow) = flow else {
        return rsx! {
            div { class: "h-full w-full overflow-y-auto",
                p { class: "p-4 font-mono text-xs text-red-500", "Invalid Ladder Diagram body" }
            }
        };
    };

    rsx! {
        div { class: "h-full w-full overflow-y-auto", style: "scrollbar-gutter:stable",
            div { class: "flex flex-1 flex-col gap-4 px-2",
                if !flow.rungs.is_empty() {
                    div { class: "h-fit rounded-lg border dark:border-neutral-800",
                        for (index, rung) in flow.rungs.iter().cloned().enumerate() {
                            LadderRungView {
                                key: "{rung.id}",
                                flow_name: flow.name.clone(),
                                rung,
                                index,
                                count: flow.rungs.len(),
                            }
                        }
                    }
                }
                CreateRung {
                    onclick: move |_| {
                        let mut next = parse_flow(&store.current_content(), &pou_name)
                            .unwrap_or_else(|_| LadderFlow { name: pou_name.clone(), rungs: Vec::new() });
                        next.rungs.push(new_rung(&pou_name));
                        write_flow(&mut store, &next);
                    }
                }
            }
        }
    }
}

#[component]
fn LadderRungView(flow_name: String, rung: LadderRung, index: usize, count: usize) -> Element {
    let mut store = use_context::<AppStore>();
    let mut open = use_signal(|| true);
    let mut editing_node = use_signal(|| None::<String>);
    let rung_id = rung.id.clone();
    let comment = rung.comment.clone();
    let body_rung = rung.clone();
    let duplicate_rung = rung.clone();
    let delete_rung_id = rung.id.clone();
    let rounded_top = index == 0;
    let rounded_bottom = index + 1 == count;
    let canvas_width = rung
        .react_flow_viewport
        .first()
        .copied()
        .unwrap_or(DEFAULT_BOUNDS[0])
        .max(DEFAULT_BOUNDS[0]);
    let canvas_height = rung
        .react_flow_viewport
        .get(1)
        .copied()
        .unwrap_or(DEFAULT_BOUNDS[1])
        .max(DEFAULT_BOUNDS[1]);

    rsx! {
        div { "aria-label": "Rung container", class: "overflow w-full", id: "{rung.id}",
            div {
                "aria-label": "Rung header",
                class: if open() {
                    if rounded_top { "flex w-full flex-row rounded-t-lg border border-transparent border-b-0 bg-neutral-50 p-1 dark:bg-neutral-900" }
                    else { "flex w-full flex-row border border-transparent border-b-0 bg-neutral-50 p-1 dark:bg-neutral-900" }
                } else if rounded_bottom {
                    "flex w-full flex-row rounded-b-lg border border-transparent bg-neutral-50 p-1 dark:bg-neutral-900"
                } else {
                    "flex w-full flex-row border border-transparent bg-neutral-50 p-1 dark:bg-neutral-900"
                },
                div { class: "flex items-center", DragHandleIcon {} }
                div { class: "flex w-full items-center rounded-lg border border-transparent px-1",
                    input {
                        class: "w-full bg-transparent px-1 text-xs text-neutral-850 outline-none dark:text-neutral-200",
                        placeholder: "Start typing to add a comment to this rung",
                        value: "{comment}",
                        oninput: {
                            let flow_name = flow_name.clone();
                            let rung_id = rung_id.clone();
                            move |event| {
                                if let Ok(mut flow) = parse_flow(&store.current_content(), &flow_name) {
                                    if let Some(rung) = flow.rungs.iter_mut().find(|candidate| candidate.id == rung_id) {
                                        rung.comment = event.value();
                                        write_flow(&mut store, &flow);
                                    }
                                }
                            }
                        },
                    }
                }
                div { class: "flex flex-row gap-1",
                    button {
                        "aria-label": "Duplicate body button",
                        class: "flex h-full w-7 items-center justify-center rounded-md p-1 hover:bg-neutral-200 dark:hover:bg-neutral-800",
                        onclick: {
                            let flow_name = flow_name.clone();
                            move |_| {
                                if let Ok(mut flow) = parse_flow(&store.current_content(), &flow_name) {
                                    let rung_index = flow.rungs.iter().position(|candidate| candidate.id == duplicate_rung.id);
                                    if let Some(rung_index) = rung_index {
                                        let copy = duplicate_rung_with_new_ids(&flow_name, &duplicate_rung);
                                        flow.rungs.insert(rung_index + 1, copy);
                                        write_flow(&mut store, &flow);
                                    }
                                }
                            }
                        },
                        DuplicateIcon {}
                    }
                    button {
                        "aria-label": "Delete body button",
                        class: "flex h-full w-7 items-center justify-center rounded-md p-1 text-brand hover:bg-neutral-200 dark:hover:bg-neutral-800",
                        onclick: {
                            let flow_name = flow_name.clone();
                            move |_| {
                                if let Ok(mut flow) = parse_flow(&store.current_content(), &flow_name) {
                                    flow.rungs.retain(|candidate| candidate.id != delete_rung_id);
                                    write_flow(&mut store, &flow);
                                }
                            }
                        },
                        "×"
                    }
                    button {
                        "aria-label": "Expand body button",
                        class: "h-fit rounded-md p-1 text-brand hover:bg-neutral-200 dark:hover:bg-neutral-800",
                        onclick: move |_| open.toggle(),
                        if open() { "⌃" } else { "⌄" }
                    }
                }
            }
            if open() {
                div {
                    "aria-label": "Rung body",
                    class: if rounded_bottom { "relative h-fit w-full rounded-b-lg border border-transparent p-1" } else { "relative h-fit w-full border border-transparent p-1" },
                    div { class: "h-full w-full overflow-x-auto",
                        GraphCanvas {
                            key: "ladder-{body_rung.id}",
                            nodes: body_rung.nodes.clone(),
                            edges: body_rung.edges.clone(),
                            width: canvas_width,
                            height: canvas_height + 8.0,
                            viewport: GraphViewport::default(),
                            allow_pan_zoom: false,
                            on_viewport: move |_| {},
                            on_move: {
                                let flow_name = flow_name.clone();
                                let rung_id = body_rung.id.clone();
                                move |(node_id, x, y)| {
                                    if let Ok(mut flow) = parse_flow(&store.current_content(), &flow_name) {
                                        if let Some(rung) = flow.rungs.iter_mut().find(|candidate| candidate.id == rung_id) {
                                            if let Some(node) = rung.nodes.iter_mut().find(|node| node.id == node_id) {
                                                if node.draggable {
                                                    node.position = GraphPosition { x: snap_grid(x), y: snap_grid(y) };
                                                    refresh_handle_positions(node);
                                                    write_flow(&mut store, &flow);
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            on_add: {
                                let flow_name = flow_name.clone();
                                let rung_id = body_rung.id.clone();
                                move |(node_type, x, y): (String, f64, f64)| {
                                    if let Ok(mut flow) = parse_flow(&store.current_content(), &flow_name) {
                                        if let Some(rung) = flow.rungs.iter_mut().find(|candidate| candidate.id == rung_id) {
                                            if insert_ladder_node(rung, &node_type, x, y) {
                                                write_flow(&mut store, &flow);
                                            }
                                        }
                                    }
                                }
                            },
                            on_delete: {
                                let flow_name = flow_name.clone();
                                let rung_id = body_rung.id.clone();
                                move |node_id: String| {
                                    if let Ok(mut flow) = parse_flow(&store.current_content(), &flow_name) {
                                        if let Some(rung) = flow.rungs.iter_mut().find(|candidate| candidate.id == rung_id) {
                                            if remove_ladder_node(rung, &node_id) {
                                                write_flow(&mut store, &flow);
                                            }
                                        }
                                    }
                                }
                            },
                            on_delete_edge: move |_| {},
                            on_copy_selection: move |_| {},
                            on_paste: move |_| {},
                            allow_edge_selection: false,
                            allow_handle_branch_drop: true,
                            on_handle_branch_drop: {
                                let flow_name = flow_name.clone();
                                let rung_id = body_rung.id.clone();
                                move |(block_id, handle_id, direction, node_type): (String, String, String, String)| {
                                    if let Ok(mut flow) = parse_flow(&store.current_content(), &flow_name) {
                                        if let Some(rung) = flow.rungs.iter_mut().find(|candidate| candidate.id == rung_id) {
                                            if insert_ladder_block_handle_branch(
                                                rung,
                                                &block_id,
                                                &handle_id,
                                                &direction,
                                                &node_type,
                                            ) {
                                                write_flow(&mut store, &flow);
                                            }
                                        }
                                    }
                                }
                            },
                            on_connect: move |_| {},
                            on_edit: move |node_id| editing_node.set(Some(node_id))
                        }
                    }
                }
                if let Some(node_id) = editing_node() {
                    if let Some(node) = rung.nodes.iter().find(|node| node.id == node_id).cloned() {
                        NodeEditor {
                            node,
                            current_pou: flow_name.clone(),
                            on_apply: {
                                let flow_name = flow_name.clone();
                                let rung_id = rung.id.clone();
                                move |updated: GraphNode| {
                                    if let Ok(mut flow) = parse_flow(&store.current_content(), &flow_name) {
                                        if let Some(rung) = flow.rungs.iter_mut().find(|candidate| candidate.id == rung_id) {
                                            let updated_id = updated.id.clone();
                                            if let Some(node) = rung.nodes.iter_mut().find(|node| node.id == updated_id) {
                                                *node = updated;
                                                refresh_handle_positions(node);
                                                sync_ladder_block_variable_nodes(rung, &updated_id);
                                                write_flow(&mut store, &flow);
                                            }
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
}

#[component]
fn CreateRung(onclick: EventHandler<MouseEvent>) -> Element {
    rsx! {
        div {
            "aria-label": "Create new rung",
            class: "flex w-full select-none flex-row items-center justify-between rounded-lg border bg-transparent p-1 opacity-40 hover:opacity-100 dark:border-neutral-800",
            p { class: "ml-2 cursor-default text-xs", "Create new rung" }
            button {
                "aria-label": "Create new rung button",
                class: "mr-1 rounded-md p-1 text-brand hover:bg-neutral-100 dark:hover:bg-neutral-900",
                onclick: move |event| onclick.call(event),
                "+"
            }
        }
    }
}

fn parse_flow(content: &str, pou_name: &str) -> Result<LadderFlow, serde_json::Error> {
    if content.trim().is_empty() {
        return Ok(LadderFlow {
            name: pou_name.to_owned(),
            rungs: Vec::new(),
        });
    }
    serde_json::from_str(content)
}

fn write_flow(store: &mut AppStore, flow: &LadderFlow) {
    if let Ok(content) = serde_json::to_string_pretty(flow) {
        store.set_current_content(content);
    }
}

fn new_rung(editor_name: &str) -> LadderRung {
    let rung_id = format!("rung_{editor_name}_{}", Uuid::new_v4());
    let left = power_rail_node(&rung_id, true);
    let right = power_rail_node(&rung_id, false);
    let edge = GraphEdge {
        id: format!("e_{}_{}", left.id, right.id),
        source: left.id.clone(),
        source_handle: "left-rail".to_owned(),
        target: right.id.clone(),
        target_handle: "right-rail".to_owned(),
        edge_type: Some("smoothstep".to_owned()),
    };
    LadderRung {
        id: rung_id,
        comment: String::new(),
        default_bounds: DEFAULT_BOUNDS.to_vec(),
        react_flow_viewport: DEFAULT_BOUNDS.to_vec(),
        nodes: vec![left, right],
        edges: vec![edge],
    }
}

fn insert_ladder_node(rung: &mut LadderRung, node_type: &str, x: f64, y: f64) -> bool {
    let x = snap_grid(x);
    let y = snap_grid(y);
    if let Some(anchor_id) = parallel_drop_anchor(rung, x, y) {
        return insert_ladder_node_parallel(rung, node_type, &anchor_id, x, y);
    }
    let Some(mut node) = new_ladder_node(node_type, x, y) else {
        return false;
    };
    let Some(edge_index) = nearest_edge_index(rung, x, y) else {
        rung.nodes.push(node);
        return true;
    };
    let edge = rung.edges.remove(edge_index);
    let input_handle = node
        .data
        .get("inputConnector")
        .and_then(|handle| handle.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("input")
        .to_owned();
    let output_handle = node
        .data
        .get("outputConnector")
        .and_then(|handle| handle.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("output")
        .to_owned();
    let node_id = node.id.clone();
    refresh_handle_positions(&mut node);
    rung.nodes.push(node);
    rung.edges.push(GraphEdge {
        id: format!(
            "e_{}_{}__{}_{}",
            edge.source, node_id, edge.source_handle, input_handle
        ),
        source: edge.source,
        source_handle: edge.source_handle,
        target: node_id.clone(),
        target_handle: input_handle,
        edge_type: edge.edge_type.clone(),
    });
    rung.edges.push(GraphEdge {
        id: format!(
            "e_{}_{}__{}_{}",
            node_id, edge.target, output_handle, edge.target_handle
        ),
        source: node_id,
        source_handle: output_handle,
        target: edge.target,
        target_handle: edge.target_handle,
        edge_type: edge.edge_type,
    });
    let width = rung
        .nodes
        .iter()
        .map(|node| node.position.x + node.width.unwrap_or(80.0) + 48.0)
        .fold(DEFAULT_BOUNDS[0], f64::max);
    let height = rung
        .nodes
        .iter()
        .map(|node| node.position.y + node.height.unwrap_or(40.0) + 32.0)
        .fold(DEFAULT_BOUNDS[1], f64::max);
    rung.react_flow_viewport = vec![width, height];
    true
}

fn parallel_drop_anchor(rung: &LadderRung, x: f64, y: f64) -> Option<String> {
    rung.nodes
        .iter()
        .filter(|node| {
            !matches!(node.node_type.as_str(), "powerRail" | "parallel")
                && rung.edges.iter().any(|edge| edge.target == node.id)
                && rung.edges.iter().any(|edge| edge.source == node.id)
        })
        .filter_map(|node| {
            let width = node.width.unwrap_or(80.0);
            let height = node.height.unwrap_or(40.0);
            let center_x = node.position.x + width / 2.0;
            let center_y = node.position.y + height / 2.0;
            let vertical = (y - center_y).abs();
            let horizontal = (x - center_x).abs();
            if !(32.0..=128.0).contains(&vertical) || horizontal > width.max(96.0) {
                return None;
            }
            Some((node.id.clone(), vertical + horizontal * 0.25))
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(id, _)| id)
}

fn insert_ladder_node_parallel(
    rung: &mut LadderRung,
    node_type: &str,
    anchor_id: &str,
    x: f64,
    y: f64,
) -> bool {
    let Some(anchor) = rung.nodes.iter().find(|node| node.id == anchor_id).cloned() else {
        return false;
    };
    let incoming = rung
        .edges
        .iter()
        .find(|edge| edge.target == anchor_id)
        .cloned();
    let anchor_output = anchor
        .data
        .get("outputConnector")
        .and_then(|handle| handle.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("output");
    let outgoing = rung
        .edges
        .iter()
        .find(|edge| edge.source == anchor_id && edge.source_handle == anchor_output)
        .cloned();
    let (Some(incoming), Some(outgoing)) = (incoming, outgoing) else {
        return false;
    };

    let branch_y = if y >= anchor.position.y {
        anchor.position.y + anchor.height.unwrap_or(40.0) + 48.0
    } else {
        anchor.position.y - 72.0
    };
    let branch_x = snap_grid(x.max(anchor.position.x));
    let Some(mut branch) = new_ladder_node(node_type, branch_x, snap_grid(branch_y)) else {
        return false;
    };
    let branch_input = node_input_handle(&branch);
    let branch_output = node_output_handle(&branch);
    refresh_handle_positions(&mut branch);

    let open_x = (anchor.position.x - 32.0).max(16.0);
    let close_x = (anchor.position.x + anchor.width.unwrap_or(80.0) + 32.0)
        .max(branch.position.x + branch.width.unwrap_or(80.0) + 32.0);
    let junction_y = anchor.position.y + anchor.height.unwrap_or(40.0) / 2.0 - 1.0;
    let open_id = format!("PARALLEL_OPEN_{}", Uuid::new_v4());
    let close_id = format!("PARALLEL_CLOSE_{}", Uuid::new_v4());
    let mut open = parallel_node(open_id.clone(), "open", open_x, junction_y);
    let mut close = parallel_node(close_id.clone(), "close", close_x, junction_y);
    if let Some(data) = open.data.as_object_mut() {
        data.insert(
            "parallelCloseReference".to_owned(),
            Value::String(close_id.clone()),
        );
    }
    if let Some(data) = close.data.as_object_mut() {
        data.insert(
            "parallelOpenReference".to_owned(),
            Value::String(open_id.clone()),
        );
    }
    refresh_handle_positions(&mut open);
    refresh_handle_positions(&mut close);

    rung.edges
        .retain(|edge| edge.id != incoming.id && edge.id != outgoing.id);
    rung.nodes.push(open.clone());
    rung.nodes.push(branch.clone());
    rung.nodes.push(close.clone());

    let anchor_input = node_input_handle(&anchor);
    let open_input = node_input_handle(&open);
    let open_output = node_output_handle(&open);
    let open_parallel_output = parallel_output_handle(&open).unwrap_or("output-down".to_owned());
    let close_input = node_input_handle(&close);
    let close_parallel_input = parallel_input_handle(&close).unwrap_or("input-down".to_owned());
    let close_output = node_output_handle(&close);
    rung.edges.extend([
        ladder_edge(
            incoming.source,
            incoming.source_handle,
            open_id.clone(),
            open_input,
            incoming.edge_type.clone(),
        ),
        ladder_edge(
            open_id.clone(),
            open_output,
            anchor.id.clone(),
            anchor_input,
            Some("smoothstep".to_owned()),
        ),
        ladder_edge(
            anchor.id.clone(),
            anchor_output.to_owned(),
            close_id.clone(),
            close_input,
            Some("smoothstep".to_owned()),
        ),
        ladder_edge(
            open_id.clone(),
            open_parallel_output,
            branch.id.clone(),
            branch_input,
            Some("smoothstep".to_owned()),
        ),
        ladder_edge(
            branch.id.clone(),
            branch_output,
            close_id.clone(),
            close_parallel_input,
            Some("smoothstep".to_owned()),
        ),
        ladder_edge(
            close_id,
            close_output,
            outgoing.target,
            outgoing.target_handle,
            outgoing.edge_type,
        ),
    ]);
    update_rung_bounds(rung);
    true
}

fn remove_ladder_node(rung: &mut LadderRung, node_id: &str) -> bool {
    let Some(node) = rung.nodes.iter().find(|node| node.id == node_id).cloned() else {
        return false;
    };
    if node.node_type == "powerRail"
        || node.data.get("deletable").and_then(Value::as_bool) == Some(false)
    {
        return false;
    }
    if collapse_block_handle_branch_after_removal(rung, node_id) {
        update_rung_bounds(rung);
        return true;
    }
    if node.node_type == "block" {
        remove_block_handle_branches(rung, node_id);
        remove_attached_block_variables(rung, node_id);
    }
    if collapse_simple_parallel_after_removal(rung, node_id) {
        update_rung_bounds(rung);
        return true;
    }

    let incoming = rung
        .edges
        .iter()
        .filter(|edge| edge.target == node_id)
        .cloned()
        .collect::<Vec<_>>();
    let outgoing = rung
        .edges
        .iter()
        .filter(|edge| edge.source == node_id)
        .cloned()
        .collect::<Vec<_>>();
    rung.nodes.retain(|node| node.id != node_id);
    rung.edges
        .retain(|edge| edge.source != node_id && edge.target != node_id);

    if incoming.len() == 1 && outgoing.len() == 1 {
        let before = &incoming[0];
        let after = &outgoing[0];
        let duplicate = rung.edges.iter().any(|edge| {
            edge.source == before.source
                && edge.source_handle == before.source_handle
                && edge.target == after.target
                && edge.target_handle == after.target_handle
        });
        if !duplicate {
            rung.edges.push(GraphEdge {
                id: format!(
                    "e_{}_{}__{}_{}",
                    before.source, after.target, before.source_handle, after.target_handle
                ),
                source: before.source.clone(),
                source_handle: before.source_handle.clone(),
                target: after.target.clone(),
                target_handle: after.target_handle.clone(),
                edge_type: before.edge_type.clone().or_else(|| after.edge_type.clone()),
            });
        }
    }
    true
}

fn collapse_block_handle_branch_after_removal(rung: &mut LadderRung, node_id: &str) -> bool {
    let incoming = rung
        .edges
        .iter()
        .find(|edge| edge.target == node_id)
        .cloned();
    let outgoing = rung
        .edges
        .iter()
        .find(|edge| edge.source == node_id)
        .cloned();
    let (Some(incoming), Some(outgoing)) = (incoming, outgoing) else {
        return false;
    };

    let input_branch = rung
        .nodes
        .iter()
        .find(|node| node.id == incoming.source)
        .is_some_and(|node| node.node_type == "powerRail")
        && rung
            .nodes
            .iter()
            .find(|node| node.id == outgoing.target)
            .is_some_and(|node| node.node_type == "block")
        && incoming.source_handle.starts_with("branch_");
    let output_branch = rung
        .nodes
        .iter()
        .find(|node| node.id == incoming.source)
        .is_some_and(|node| node.node_type == "block")
        && rung
            .nodes
            .iter()
            .find(|node| node.id == outgoing.target)
            .is_some_and(|node| node.node_type == "powerRail")
        && outgoing.target_handle.starts_with("branch_");
    if !input_branch && !output_branch {
        return false;
    }

    let (block_id, handle_id, rail_id, branch_handle_id, direction) = if input_branch {
        (
            outgoing.target.clone(),
            outgoing.target_handle.clone(),
            incoming.source.clone(),
            incoming.source_handle.clone(),
            "input",
        )
    } else {
        (
            incoming.source.clone(),
            incoming.source_handle.clone(),
            outgoing.target.clone(),
            outgoing.target_handle.clone(),
            "output",
        )
    };
    if branch_handle_id != format!("branch_{block_id}_{handle_id}") {
        return false;
    }

    rung.nodes.retain(|node| node.id != node_id);
    rung.edges
        .retain(|edge| edge.source != node_id && edge.target != node_id);
    if let Some(rail) = rung.nodes.iter_mut().find(|node| node.id == rail_id) {
        remove_rail_branch_handle(rail, &branch_handle_id, direction);
    }
    sync_ladder_block_variable_nodes(rung, &block_id);
    true
}

fn remove_block_handle_branches(rung: &mut LadderRung, block_id: &str) {
    let branch_nodes = rung
        .edges
        .iter()
        .filter_map(|edge| {
            if edge.target == block_id && edge.source_handle.starts_with("branch_") {
                None
            } else if edge.target == block_id {
                let source = rung.nodes.iter().find(|node| node.id == edge.source)?;
                let incoming = rung
                    .edges
                    .iter()
                    .find(|incoming| incoming.target == source.id)?;
                (source.node_type != "variable" && incoming.source_handle.starts_with("branch_"))
                    .then_some(source.id.clone())
            } else if edge.source == block_id {
                let target = rung.nodes.iter().find(|node| node.id == edge.target)?;
                let outgoing = rung
                    .edges
                    .iter()
                    .find(|outgoing| outgoing.source == target.id)?;
                (target.node_type != "variable" && outgoing.target_handle.starts_with("branch_"))
                    .then_some(target.id.clone())
            } else {
                None
            }
        })
        .collect::<HashSet<_>>();
    for node_id in branch_nodes {
        let _ = collapse_block_handle_branch_after_removal(rung, &node_id);
    }
}

fn collapse_simple_parallel_after_removal(rung: &mut LadderRung, node_id: &str) -> bool {
    let Some(incoming) = rung
        .edges
        .iter()
        .find(|edge| edge.target == node_id)
        .cloned()
    else {
        return false;
    };
    let Some(outgoing) = rung
        .edges
        .iter()
        .find(|edge| edge.source == node_id)
        .cloned()
    else {
        return false;
    };
    let Some(open) = rung
        .nodes
        .iter()
        .find(|node| node.id == incoming.source && parallel_side(node) == Some("open"))
        .cloned()
    else {
        return false;
    };
    let Some(close) = rung
        .nodes
        .iter()
        .find(|node| node.id == outgoing.target && parallel_side(node) == Some("close"))
        .cloned()
    else {
        return false;
    };
    let open_ref = open
        .data
        .get("parallelCloseReference")
        .and_then(Value::as_str);
    let close_ref = close
        .data
        .get("parallelOpenReference")
        .and_then(Value::as_str);
    if open_ref != Some(close.id.as_str()) || close_ref != Some(open.id.as_str()) {
        return false;
    }

    let Some(predecessor) = rung
        .edges
        .iter()
        .find(|edge| edge.target == open.id && edge.source != node_id)
        .cloned()
    else {
        return false;
    };
    let Some(successor) = rung
        .edges
        .iter()
        .find(|edge| edge.source == close.id && edge.target != node_id)
        .cloned()
    else {
        return false;
    };
    let sibling_in = rung
        .edges
        .iter()
        .find(|edge| edge.source == open.id && edge.target != node_id)
        .cloned();
    let sibling_out = rung
        .edges
        .iter()
        .find(|edge| edge.target == close.id && edge.source != node_id)
        .cloned();
    let (Some(sibling_in), Some(sibling_out)) = (sibling_in, sibling_out) else {
        return false;
    };
    if sibling_in.target != sibling_out.source {
        return false;
    }
    let sibling_id = sibling_in.target.clone();

    rung.nodes
        .retain(|node| node.id != node_id && node.id != open.id && node.id != close.id);
    rung.edges.retain(|edge| {
        edge.source != node_id
            && edge.target != node_id
            && edge.source != open.id
            && edge.target != open.id
            && edge.source != close.id
            && edge.target != close.id
    });
    rung.edges.extend([
        ladder_edge(
            predecessor.source,
            predecessor.source_handle,
            sibling_id.clone(),
            sibling_in.target_handle,
            predecessor.edge_type.clone(),
        ),
        ladder_edge(
            sibling_id,
            sibling_out.source_handle,
            successor.target,
            successor.target_handle,
            successor.edge_type,
        ),
    ]);
    true
}

fn nearest_edge_index(rung: &LadderRung, x: f64, y: f64) -> Option<usize> {
    rung.edges
        .iter()
        .enumerate()
        .filter_map(|(index, edge)| {
            let source = rung.nodes.iter().find(|node| node.id == edge.source)?;
            let target = rung.nodes.iter().find(|node| node.id == edge.target)?;
            let (sx, sy) = node_handle_point(source, &edge.source_handle, true);
            let (tx, ty) = node_handle_point(target, &edge.target_handle, false);
            let mid_x = (sx + tx) / 2.0;
            let mid_y = (sy + ty) / 2.0;
            let outside = if x < sx.min(tx) {
                sx.min(tx) - x
            } else if x > sx.max(tx) {
                x - sx.max(tx)
            } else {
                0.0
            };
            let score = (mid_y - y).abs() * 2.0 + (mid_x - x).abs() + outside * 4.0;
            Some((index, score))
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .map(|(index, _)| index)
}

fn new_ladder_node(node_type: &str, x: f64, y: f64) -> Option<GraphNode> {
    let id = format!("{}_{}", node_type.to_ascii_uppercase(), Uuid::new_v4());
    match node_type {
        "contact" => Some(ladder_contact_or_coil(id, "contact", x, y, 24.0, 24.0)),
        "coil" => Some(ladder_contact_or_coil(id, "coil", x, y, 28.0, 24.0)),
        "block" => Some(ladder_block(id, x, y)),
        _ => None,
    }
}

fn ladder_contact_or_coil(
    id: String,
    node_type: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> GraphNode {
    let input = graph_handle(
        "input",
        "left",
        "target",
        (x, y + height / 2.0),
        (0.0, height / 2.0),
        false,
    );
    let output = graph_handle(
        "output",
        "right",
        "source",
        (x + width, y + height / 2.0),
        (width, height / 2.0),
        false,
    );
    GraphNode {
        id,
        node_type: node_type.to_owned(),
        position: GraphPosition { x, y },
        height: Some(height),
        width: Some(width),
        measured: Some(GraphMeasured { width, height }),
        draggable: true,
        selectable: true,
        data: json!({
            "handles": [input.clone(), output.clone()],
            "variant": "default",
            "inputHandles": [input.clone()],
            "outputHandles": [output.clone()],
            "inputConnector": input,
            "outputConnector": output,
            "numericId": numeric_id(),
            "variable": { "name": "" },
            "executionOrder": 0,
            "draggable": true,
            "selectable": true,
            "deletable": true
        }),
    }
}

fn ladder_block(id: String, x: f64, y: f64) -> GraphNode {
    let width = 216.0;
    let height = 128.0;
    let variant = json!({
        "name": "???",
        "type": "generic",
        "variables": [
            { "name": "???", "class": "input", "type": { "definition": "base-type", "value": "BOOL" } },
            { "name": "???", "class": "output", "type": { "definition": "base-type", "value": "BOOL" } }
        ],
        "documentation": ""
    });
    let input = graph_handle("???", "left", "target", (x, y + 36.0), (0.0, 36.0), false);
    let output = graph_handle(
        "???",
        "right",
        "source",
        (x + width, y + 36.0),
        (width, 36.0),
        false,
    );
    GraphNode {
        id,
        node_type: "block".to_owned(),
        position: GraphPosition { x, y },
        height: Some(height),
        width: Some(width),
        measured: Some(GraphMeasured { width, height }),
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
            "lockExecutionControl": false,
            "connectedVariables": [],
            "draggable": true,
            "selectable": true,
            "deletable": true
        }),
    }
}

fn insert_ladder_block_handle_branch(
    rung: &mut LadderRung,
    block_id: &str,
    handle_id: &str,
    direction: &str,
    node_type: &str,
) -> bool {
    if !matches!(node_type, "contact" | "coil") || !matches!(direction, "input" | "output") {
        return false;
    }
    let Some(block) = rung
        .nodes
        .iter()
        .find(|node| node.id == block_id && node.node_type == "block")
        .cloned()
    else {
        return false;
    };
    if !block_handle_is_bool(&block, handle_id) {
        return false;
    }
    let handles_key = if direction == "input" {
        "inputHandles"
    } else {
        "outputHandles"
    };
    let Some((handle_index, handle)) = block
        .data
        .get(handles_key)
        .and_then(Value::as_array)
        .and_then(|handles| {
            handles
                .iter()
                .enumerate()
                .find(|(_, handle)| handle.get("id").and_then(Value::as_str) == Some(handle_id))
        })
    else {
        return false;
    };
    if handle_index == 0 || block_handle_has_branch(rung, block_id, handle_id, direction) {
        return false;
    }
    let handle_y = block.position.y
        + handle
            .get("relPosition")
            .and_then(|position| position.get("y"))
            .and_then(Value::as_f64)
            .unwrap_or(block.height.unwrap_or(128.0) / 2.0);

    let variable_edge = if direction == "input" {
        rung.edges
            .iter()
            .find(|edge| edge.target == block_id && edge.target_handle == handle_id)
            .cloned()
    } else {
        rung.edges
            .iter()
            .find(|edge| edge.source == block_id && edge.source_handle == handle_id)
            .cloned()
    };
    if let Some(edge) = variable_edge {
        let variable_id = if direction == "input" {
            edge.source.clone()
        } else {
            edge.target.clone()
        };
        let is_variable = rung
            .nodes
            .iter()
            .any(|node| node.id == variable_id && node.node_type == "variable");
        if is_variable {
            rung.nodes.retain(|node| node.id != variable_id);
            rung.edges.retain(|candidate| candidate.id != edge.id);
        }
    }

    let rail_index = rung.nodes.iter().position(|node| {
        node.node_type == "powerRail"
            && node.data.get("variant").and_then(Value::as_str)
                == Some(if direction == "input" {
                    "left"
                } else {
                    "right"
                })
    });
    let Some(rail_index) = rail_index else {
        return false;
    };
    let branch_handle_id = format!("branch_{block_id}_{handle_id}");
    let rail_id = rung.nodes[rail_index].id.clone();
    add_rail_branch_handle(
        &mut rung.nodes[rail_index],
        &branch_handle_id,
        direction,
        handle_y,
    );

    let branch_x = if direction == "input" {
        ((rung.nodes[rail_index].position.x + block.position.x) / 2.0).max(48.0)
    } else {
        block.position.x + block.width.unwrap_or(216.0) + 96.0
    };
    let branch_y = handle_y - 12.0;
    let Some(mut branch) = new_ladder_node(node_type, snap_grid(branch_x), snap_grid(branch_y))
    else {
        return false;
    };
    let branch_id = branch.id.clone();
    let input_handle = node_input_handle(&branch);
    let output_handle = node_output_handle(&branch);
    refresh_handle_positions(&mut branch);
    rung.nodes.push(branch);

    if direction == "input" {
        rung.edges.extend([
            ladder_edge(
                rail_id,
                branch_handle_id,
                branch_id.clone(),
                input_handle,
                Some("smoothstep".to_owned()),
            ),
            ladder_edge(
                branch_id,
                output_handle,
                block_id.to_owned(),
                handle_id.to_owned(),
                Some("smoothstep".to_owned()),
            ),
        ]);
    } else {
        rung.edges.extend([
            ladder_edge(
                block_id.to_owned(),
                handle_id.to_owned(),
                branch_id.clone(),
                input_handle,
                Some("smoothstep".to_owned()),
            ),
            ladder_edge(
                branch_id,
                output_handle,
                rail_id,
                branch_handle_id,
                Some("smoothstep".to_owned()),
            ),
        ]);
    }
    update_rung_bounds(rung);
    true
}

fn block_handle_has_branch(
    rung: &LadderRung,
    block_id: &str,
    handle_id: &str,
    direction: &str,
) -> bool {
    if direction == "input" {
        rung.edges.iter().any(|edge| {
            edge.target == block_id
                && edge.target_handle == handle_id
                && rung
                    .nodes
                    .iter()
                    .find(|node| node.id == edge.source)
                    .is_some_and(|node| node.node_type != "variable")
        })
    } else {
        rung.edges.iter().any(|edge| {
            edge.source == block_id
                && edge.source_handle == handle_id
                && rung
                    .nodes
                    .iter()
                    .find(|node| node.id == edge.target)
                    .is_some_and(|node| node.node_type != "variable")
        })
    }
}

fn block_handle_is_bool(block: &GraphNode, handle_id: &str) -> bool {
    block
        .data
        .get("variant")
        .and_then(|variant| variant.get("variables"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|variable| {
            variable.get("name").and_then(Value::as_str) == Some(handle_id)
                && variable
                    .get("type")
                    .and_then(|variable_type| variable_type.get("value"))
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.eq_ignore_ascii_case("BOOL"))
        })
}

fn add_rail_branch_handle(rail: &mut GraphNode, handle_id: &str, direction: &str, global_y: f64) {
    remove_rail_branch_handle(rail, handle_id, direction);
    let left = direction == "input";
    let relative_y = global_y - rail.position.y;
    let relative_x = if left { rail.width.unwrap_or(3.0) } else { 0.0 };
    let handle = graph_handle(
        handle_id,
        if left { "right" } else { "left" },
        if left { "source" } else { "target" },
        (rail.position.x + relative_x, global_y),
        (relative_x, relative_y),
        false,
    );
    if let Some(handles) = rail.data.get_mut("handles").and_then(Value::as_array_mut) {
        handles.push(handle.clone());
    }
    let key = if left {
        "outputHandles"
    } else {
        "inputHandles"
    };
    if let Some(handles) = rail.data.get_mut(key).and_then(Value::as_array_mut) {
        handles.push(handle);
    }
}

fn remove_rail_branch_handle(rail: &mut GraphNode, handle_id: &str, direction: &str) {
    for key in [
        "handles",
        if direction == "input" {
            "outputHandles"
        } else {
            "inputHandles"
        },
    ] {
        if let Some(handles) = rail.data.get_mut(key).and_then(Value::as_array_mut) {
            handles.retain(|handle| handle.get("id").and_then(Value::as_str) != Some(handle_id));
        }
    }
}

fn sync_ladder_block_variable_nodes(rung: &mut LadderRung, block_id: &str) {
    let Some(block) = rung
        .nodes
        .iter()
        .find(|node| node.id == block_id && node.node_type == "block")
        .cloned()
    else {
        return;
    };

    let mut previous = HashMap::<String, GraphNode>::new();
    let attached_ids = rung
        .nodes
        .iter()
        .filter(|node| {
            node.node_type == "variable"
                && node
                    .data
                    .get("block")
                    .and_then(|block| block.get("id"))
                    .and_then(Value::as_str)
                    == Some(block_id)
        })
        .map(|node| {
            let variant = node
                .data
                .get("variant")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let handle = node
                .data
                .get("block")
                .and_then(|block| block.get("handleId"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            previous.insert(format!("{variant}::{handle}"), node.clone());
            node.id.clone()
        })
        .collect::<Vec<_>>();
    rung.nodes.retain(|node| !attached_ids.contains(&node.id));
    rung.edges.retain(|edge| {
        !attached_ids.contains(&edge.source) && !attached_ids.contains(&edge.target)
    });

    let variant_variables = block
        .data
        .get("variant")
        .and_then(|variant| variant.get("variables"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let variable_type_for = |handle_id: &str| {
        variant_variables
            .iter()
            .find(|variable| variable.get("name").and_then(Value::as_str) == Some(handle_id))
            .cloned()
            .unwrap_or_else(|| {
                json!({
                    "name": handle_id,
                    "class": "",
                    "type": { "definition": "base-type", "value": "BOOL" }
                })
            })
    };

    let input_handles = block
        .data
        .get("inputHandles")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let output_handles = block
        .data
        .get("outputHandles")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    for handle in input_handles.into_iter().skip(1) {
        let Some(handle_id) = handle.get("id").and_then(Value::as_str) else {
            continue;
        };
        if block_handle_has_branch(rung, block_id, handle_id, "input") {
            continue;
        }
        let rel_y = handle
            .get("relPosition")
            .and_then(|position| position.get("y"))
            .and_then(Value::as_f64)
            .unwrap_or(16.0);
        let key = format!("input::{handle_id}");
        let old = previous.remove(&key);
        let node = ladder_variable_node(
            old.as_ref(),
            &block,
            handle_id,
            "input",
            variable_type_for(handle_id),
            block.position.x - 104.0,
            block.position.y + rel_y - 16.0,
        );
        let node_id = node.id.clone();
        rung.nodes.push(node);
        rung.edges.push(ladder_edge(
            node_id,
            "output".to_owned(),
            block.id.clone(),
            handle_id.to_owned(),
            Some("smoothstep".to_owned()),
        ));
    }

    for handle in output_handles.into_iter().skip(1) {
        let Some(handle_id) = handle.get("id").and_then(Value::as_str) else {
            continue;
        };
        if block_handle_has_branch(rung, block_id, handle_id, "output") {
            continue;
        }
        let rel_y = handle
            .get("relPosition")
            .and_then(|position| position.get("y"))
            .and_then(Value::as_f64)
            .unwrap_or(16.0);
        let key = format!("output::{handle_id}");
        let old = previous.remove(&key);
        let node = ladder_variable_node(
            old.as_ref(),
            &block,
            handle_id,
            "output",
            variable_type_for(handle_id),
            block.position.x + block.width.unwrap_or(216.0) + 24.0,
            block.position.y + rel_y - 16.0,
        );
        let node_id = node.id.clone();
        rung.nodes.push(node);
        rung.edges.push(ladder_edge(
            block.id.clone(),
            handle_id.to_owned(),
            node_id,
            "input".to_owned(),
            Some("smoothstep".to_owned()),
        ));
    }
    update_rung_bounds(rung);
}

fn ladder_variable_node(
    previous: Option<&GraphNode>,
    block: &GraphNode,
    handle_id: &str,
    variant: &str,
    variable_type: Value,
    x: f64,
    y: f64,
) -> GraphNode {
    let width = 80.0;
    let height = 32.0;
    let id = previous
        .map(|node| node.id.clone())
        .unwrap_or_else(|| format!("VARIABLE_{}", Uuid::new_v4()));
    let variable = previous
        .and_then(|node| node.data.get("variable"))
        .cloned()
        .unwrap_or_else(|| json!({ "name": "" }));
    let numeric_id = previous
        .and_then(|node| node.data.get("numericId"))
        .cloned()
        .unwrap_or_else(|| Value::String(numeric_id()));
    let input = (variant == "output").then(|| {
        graph_handle(
            "input",
            "left",
            "target",
            (x, y + height / 2.0),
            (0.0, height / 2.0),
            false,
        )
    });
    let output = (variant == "input").then(|| {
        graph_handle(
            "output",
            "right",
            "source",
            (x + width, y + height / 2.0),
            (width, height / 2.0),
            false,
        )
    });
    let handles = [input.clone(), output.clone()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    GraphNode {
        id,
        node_type: "variable".to_owned(),
        position: GraphPosition { x, y },
        height: Some(height),
        width: Some(width),
        measured: Some(GraphMeasured { width, height }),
        draggable: false,
        selectable: true,
        data: json!({
            "handles": handles,
            "inputHandles": input.clone().into_iter().collect::<Vec<_>>(),
            "outputHandles": output.clone().into_iter().collect::<Vec<_>>(),
            "inputConnector": input,
            "outputConnector": output,
            "numericId": numeric_id,
            "variable": variable,
            "executionOrder": 0,
            "variant": variant,
            "block": {
                "id": block.id,
                "handleId": handle_id,
                "variableType": variable_type
            },
            "draggable": false,
            "selectable": true,
            "deletable": false
        }),
    }
}

fn remove_attached_block_variables(rung: &mut LadderRung, block_id: &str) {
    let ids = rung
        .nodes
        .iter()
        .filter(|node| {
            node.node_type == "variable"
                && node
                    .data
                    .get("block")
                    .and_then(|block| block.get("id"))
                    .and_then(Value::as_str)
                    == Some(block_id)
        })
        .map(|node| node.id.clone())
        .collect::<Vec<_>>();
    rung.nodes.retain(|node| !ids.contains(&node.id));
    rung.edges
        .retain(|edge| !ids.contains(&edge.source) && !ids.contains(&edge.target));
}

fn parallel_node(id: String, side: &str, x: f64, y: f64) -> GraphNode {
    let width = 4.0;
    let height = 2.0;
    let input = graph_handle("input", "left", "target", (x, y + 1.0), (0.0, 1.0), false);
    let output_right = graph_handle(
        "output-right",
        "right",
        "source",
        (x + width, y + 1.0),
        (width, 1.0),
        false,
    );
    let down = graph_handle(
        if side == "open" {
            "output-down"
        } else {
            "input-down"
        },
        "bottom",
        if side == "open" { "source" } else { "target" },
        (x + width / 2.0, y + height),
        (width / 2.0, height),
        false,
    );
    let top = graph_handle(
        if side == "close" {
            "output-top"
        } else {
            "input-top"
        },
        "top",
        if side == "close" { "source" } else { "target" },
        (x + width / 2.0, y),
        (width / 2.0, 0.0),
        false,
    );
    let (input_handles, output_handles, parallel_input, parallel_output) = if side == "open" {
        (
            vec![input.clone(), top.clone()],
            vec![output_right.clone(), down.clone()],
            top.clone(),
            down.clone(),
        )
    } else {
        (
            vec![input.clone(), down.clone()],
            vec![output_right.clone(), top.clone()],
            down.clone(),
            top.clone(),
        )
    };
    GraphNode {
        id,
        node_type: "parallel".to_owned(),
        position: GraphPosition { x, y },
        height: Some(height),
        width: Some(width),
        measured: Some(GraphMeasured { width, height }),
        draggable: false,
        selectable: false,
        data: json!({
            "handles": [input.clone(), output_right.clone(), down, top],
            "inputHandles": input_handles,
            "outputHandles": output_handles,
            "inputConnector": input,
            "outputConnector": output_right,
            "parallelInputConnector": parallel_input,
            "parallelOutputConnector": parallel_output,
            "parallelOpenReference": null,
            "parallelCloseReference": null,
            "numericId": numeric_id(),
            "type": side,
            "variable": { "name": "" },
            "executionOrder": 0,
            "draggable": false,
            "selectable": false,
            "deletable": false
        }),
    }
}

fn parallel_side(node: &GraphNode) -> Option<&str> {
    (node.node_type == "parallel")
        .then(|| node.data.get("type").and_then(Value::as_str))
        .flatten()
}

fn node_input_handle(node: &GraphNode) -> String {
    node.data
        .get("inputConnector")
        .and_then(|handle| handle.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("input")
        .to_owned()
}

fn node_output_handle(node: &GraphNode) -> String {
    node.data
        .get("outputConnector")
        .and_then(|handle| handle.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("output")
        .to_owned()
}

fn parallel_input_handle(node: &GraphNode) -> Option<String> {
    node.data
        .get("parallelInputConnector")
        .and_then(|handle| handle.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn parallel_output_handle(node: &GraphNode) -> Option<String> {
    node.data
        .get("parallelOutputConnector")
        .and_then(|handle| handle.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn ladder_edge(
    source: String,
    source_handle: String,
    target: String,
    target_handle: String,
    edge_type: Option<String>,
) -> GraphEdge {
    GraphEdge {
        id: format!("e_{source}_{target}__{source_handle}_{target_handle}"),
        source,
        source_handle,
        target,
        target_handle,
        edge_type,
    }
}

fn update_rung_bounds(rung: &mut LadderRung) {
    let width = rung
        .nodes
        .iter()
        .map(|node| node.position.x + node.width.unwrap_or(80.0) + 48.0)
        .fold(DEFAULT_BOUNDS[0], f64::max);
    let height = rung
        .nodes
        .iter()
        .map(|node| node.position.y + node.height.unwrap_or(40.0) + 32.0)
        .fold(DEFAULT_BOUNDS[1], f64::max);
    rung.react_flow_viewport = vec![width, height];
}

fn graph_handle(
    id: &str,
    position: &str,
    kind: &str,
    global: (f64, f64),
    relative: (f64, f64),
    connectable: bool,
) -> Value {
    json!({
        "id": id,
        "position": position,
        "type": kind,
        "isConnectable": connectable,
        "glbPosition": { "x": global.0, "y": global.1 },
        "relPosition": { "x": relative.0, "y": relative.1 }
    })
}

fn refresh_handle_positions(node: &mut GraphNode) {
    let x = node.position.x;
    let y = node.position.y;
    for key in ["handles", "inputHandles", "outputHandles"] {
        if let Some(handles) = node.data.get_mut(key).and_then(Value::as_array_mut) {
            for handle in handles {
                if let Some(rel) = handle.get("relPosition").and_then(Value::as_object) {
                    let rel_x = rel.get("x").and_then(Value::as_f64).unwrap_or(0.0);
                    let rel_y = rel.get("y").and_then(Value::as_f64).unwrap_or(0.0);
                    if let Some(object) = handle.as_object_mut() {
                        object.insert(
                            "glbPosition".to_owned(),
                            json!({ "x": x + rel_x, "y": y + rel_y }),
                        );
                    }
                }
            }
        }
    }
    for key in [
        "inputConnector",
        "outputConnector",
        "parallelInputConnector",
        "parallelOutputConnector",
    ] {
        if let Some(handle) = node.data.get_mut(key) {
            if let Some(rel) = handle.get("relPosition").and_then(Value::as_object) {
                let rel_x = rel.get("x").and_then(Value::as_f64).unwrap_or(0.0);
                let rel_y = rel.get("y").and_then(Value::as_f64).unwrap_or(0.0);
                if let Some(object) = handle.as_object_mut() {
                    object.insert(
                        "glbPosition".to_owned(),
                        json!({ "x": x + rel_x, "y": y + rel_y }),
                    );
                }
            }
        }
    }
}

fn node_handle_point(node: &GraphNode, handle_id: &str, source: bool) -> (f64, f64) {
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
        .map(|rel| {
            (
                node.position.x + rel.get("x").and_then(Value::as_f64).unwrap_or(0.0),
                node.position.y + rel.get("y").and_then(Value::as_f64).unwrap_or(0.0),
            )
        })
        .unwrap_or((
            node.position.x + node.width.unwrap_or(80.0) / 2.0,
            node.position.y + node.height.unwrap_or(40.0) / 2.0,
        ))
}

fn snap_grid(value: f64) -> f64 {
    (value / 16.0).round() * 16.0
}

fn power_rail_node(rung_id: &str, left: bool) -> GraphNode {
    let id = if left {
        format!("left-rail-{rung_id}")
    } else {
        format!("right-rail-{rung_id}")
    };
    let x = if left { 0.0 } else { DEFAULT_BOUNDS[0] };
    let handle_x = if left { 3.0 } else { DEFAULT_BOUNDS[0] - 3.0 };
    let handle_id = if left { "left-rail" } else { "right-rail" };
    let handle = json!({
        "id": handle_id,
        "position": if left { "right" } else { "left" },
        "type": if left { "source" } else { "target" },
        "isConnectable": false,
        "glbPosition": { "x": handle_x, "y": 50.0 },
        "relPosition": { "x": 3.0, "y": 20.0 }
    });
    let data = if left {
        json!({
            "handles": [handle.clone()],
            "inputHandles": [],
            "outputHandles": [handle.clone()],
            "inputConnector": null,
            "outputConnector": handle,
            "numericId": numeric_id(),
            "variant": "left",
            "variable": { "name": "" },
            "executionOrder": 0,
            "draggable": false,
            "selectable": false,
            "deletable": false
        })
    } else {
        json!({
            "handles": [handle.clone()],
            "inputHandles": [handle.clone()],
            "outputHandles": [],
            "inputConnector": handle,
            "outputConnector": null,
            "numericId": numeric_id(),
            "variant": "right",
            "variable": { "name": "" },
            "executionOrder": 0,
            "draggable": false,
            "selectable": false,
            "deletable": false
        })
    };
    GraphNode {
        id,
        node_type: "powerRail".to_owned(),
        position: GraphPosition {
            x,
            y: DEFAULT_BOUNDS[1] / 2.0 - 20.0,
        },
        height: Some(40.0),
        width: Some(3.0),
        measured: Some(GraphMeasured {
            width: 3.0,
            height: 40.0,
        }),
        draggable: false,
        selectable: false,
        data,
    }
}

fn duplicate_rung_with_new_ids(editor_name: &str, rung: &LadderRung) -> LadderRung {
    let mut node_id_map = HashMap::new();
    for node in &rung.nodes {
        let new_id = if node.node_type == "powerRail" {
            node.id.clone()
        } else {
            format!("{}_{}", node.node_type.to_ascii_uppercase(), Uuid::new_v4())
        };
        node_id_map.insert(node.id.clone(), new_id);
    }

    let mut nodes = rung.nodes.clone();
    for node in &mut nodes {
        if let Some(id) = node_id_map.get(&node.id) {
            node.id = id.clone();
        }
        restamp_numeric_id(&mut node.data);
    }

    let edges = rung
        .edges
        .iter()
        .map(|edge| {
            let source = node_id_map
                .get(&edge.source)
                .cloned()
                .unwrap_or_else(|| edge.source.clone());
            let target = node_id_map
                .get(&edge.target)
                .cloned()
                .unwrap_or_else(|| edge.target.clone());
            GraphEdge {
                id: format!(
                    "e_{source}_{target}__{}_{}",
                    edge.source_handle, edge.target_handle
                ),
                source,
                source_handle: edge.source_handle.clone(),
                target,
                target_handle: edge.target_handle.clone(),
                edge_type: edge.edge_type.clone(),
            }
        })
        .collect();

    LadderRung {
        id: format!("rung_{editor_name}_{}", Uuid::new_v4()),
        comment: rung.comment.clone(),
        default_bounds: rung.default_bounds.clone(),
        react_flow_viewport: rung.react_flow_viewport.clone(),
        nodes,
        edges,
    }
}

fn restamp_numeric_id(data: &mut Value) {
    if let Some(object) = data.as_object_mut() {
        if object.contains_key("numericId") {
            object.insert("numericId".to_owned(), Value::String(numeric_id()));
        }
    }
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
    use open_rust_plc_compiler::graphical::emit_ld_value;
    use serde_json::to_value;

    use super::{
        insert_ladder_node, new_rung, remove_ladder_node, sync_ladder_block_variable_nodes,
    };
    use crate::plc_model::{
        LadderFlow, ParsedPou, PlcVariable, PlcVariableType, PouBody, PouInterface, PouLanguage,
        PouType, VariableClass,
    };
    use crate::workspace::editor::graphical::node::{
        apply_block_variant, standard_function_block_pous,
    };

    fn bool_variable(name: &str, class: VariableClass) -> PlcVariable {
        PlcVariable {
            name: name.to_owned(),
            class: Some(class),
            variable_type: PlcVariableType::base("BOOL"),
            location: String::new(),
            initial_value: None,
            documentation: String::new(),
            debug: Some(false),
        }
    }

    fn branchable_function_block() -> ParsedPou {
        ParsedPou {
            name: "Gate".to_owned(),
            pou_type: PouType::FunctionBlock,
            interface: PouInterface {
                return_type: None,
                variables: vec![
                    bool_variable("IN", VariableClass::Input),
                    bool_variable("AUX", VariableClass::Input),
                    bool_variable("Q", VariableClass::Output),
                    bool_variable("AUXQ", VariableClass::Output),
                ],
            },
            body: PouBody::text(PouLanguage::St, ""),
            documentation: String::new(),
        }
    }

    #[test]
    fn ladder_drop_splits_the_existing_serial_edge() {
        let mut rung = new_rung("main");
        let left = rung.nodes[0].id.clone();
        let right = rung.nodes[1].id.clone();
        assert_eq!(rung.edges.len(), 1);
        assert!(insert_ladder_node(&mut rung, "contact", 144.0, 48.0));
        assert_eq!(rung.nodes.len(), 3);
        assert_eq!(rung.edges.len(), 2);
        let contact = rung
            .nodes
            .iter()
            .find(|node| node.node_type == "contact")
            .unwrap();
        assert!(rung.edges.iter().any(|edge| {
            edge.source == left && edge.target == contact.id && edge.target_handle == "input"
        }));
        assert!(rung.edges.iter().any(|edge| {
            edge.source == contact.id && edge.source_handle == "output" && edge.target == right
        }));
    }

    #[test]
    fn ladder_block_pin_variables_follow_the_selected_ton_signature() {
        let mut rung = new_rung("main");
        assert!(insert_ladder_node(&mut rung, "block", 220.0, 48.0));
        let block_id = rung
            .nodes
            .iter()
            .find(|node| node.node_type == "block")
            .unwrap()
            .id
            .clone();
        let ton = standard_function_block_pous()
            .into_iter()
            .find(|pou| pou.name.eq_ignore_ascii_case("TON"))
            .unwrap();
        {
            let block = rung
                .nodes
                .iter_mut()
                .find(|node| node.id == block_id)
                .unwrap();
            apply_block_variant(block, &ton, "timer0");
            super::refresh_handle_positions(block);
        }
        sync_ladder_block_variable_nodes(&mut rung, &block_id);

        let pin_nodes = rung
            .nodes
            .iter()
            .filter(|node| node.node_type == "variable" && node.data["block"]["id"] == block_id)
            .collect::<Vec<_>>();
        assert_eq!(pin_nodes.len(), 2);
        assert!(pin_nodes.iter().any(|node| {
            node.data["variant"] == "input" && node.data["block"]["handleId"] == "PT"
        }));
        assert!(pin_nodes.iter().any(|node| {
            node.data["variant"] == "output" && node.data["block"]["handleId"] == "ET"
        }));
        assert!(rung
            .edges
            .iter()
            .any(|edge| { edge.target == block_id && edge.target_handle == "PT" }));
        assert!(rung
            .edges
            .iter()
            .any(|edge| { edge.source == block_id && edge.source_handle == "ET" }));

        let pt_id = rung
            .nodes
            .iter()
            .find(|node| node.data["block"]["handleId"] == "PT")
            .unwrap()
            .id
            .clone();
        rung.nodes
            .iter_mut()
            .find(|node| node.id == pt_id)
            .unwrap()
            .data["variable"]["name"] = serde_json::Value::String("T#500ms".to_owned());
        let et_id = rung
            .nodes
            .iter()
            .find(|node| node.data["block"]["handleId"] == "ET")
            .unwrap()
            .id
            .clone();
        rung.nodes
            .iter_mut()
            .find(|node| node.id == et_id)
            .unwrap()
            .data["variable"]["name"] = serde_json::Value::String("elapsed".to_owned());

        sync_ladder_block_variable_nodes(&mut rung, &block_id);
        assert_eq!(
            rung.nodes
                .iter()
                .find(|node| node.data["block"]["handleId"] == "PT")
                .unwrap()
                .id,
            pt_id
        );
        assert_eq!(
            rung.nodes
                .iter()
                .find(|node| node.id == pt_id)
                .unwrap()
                .data["variable"]["name"],
            "T#500ms"
        );
        assert_eq!(
            rung.nodes
                .iter()
                .find(|node| node.data["block"]["handleId"] == "ET")
                .unwrap()
                .id,
            et_id
        );

        let emitted = emit_ld_value(
            &to_value(LadderFlow {
                name: "main".to_owned(),
                rungs: vec![rung],
            })
            .unwrap(),
        )
        .unwrap();
        assert!(emitted.warnings.is_empty(), "{:?}", emitted.warnings);
        assert!(emitted.body_st.contains("timer0"), "{}", emitted.body_st);
        assert!(
            emitted.body_st.contains("PT := T#500ms"),
            "{}",
            emitted.body_st
        );
        assert!(emitted.body_st.contains("elapsed"), "{}", emitted.body_st);
    }

    #[test]
    fn ladder_bool_block_handle_branches_replace_and_restore_pin_variables() {
        let mut rung = new_rung("main");
        assert!(insert_ladder_node(&mut rung, "block", 220.0, 48.0));
        let block_id = rung
            .nodes
            .iter()
            .find(|node| node.node_type == "block")
            .unwrap()
            .id
            .clone();
        let gate = branchable_function_block();
        {
            let block = rung
                .nodes
                .iter_mut()
                .find(|node| node.id == block_id)
                .unwrap();
            apply_block_variant(block, &gate, "gate0");
            super::refresh_handle_positions(block);
        }
        sync_ladder_block_variable_nodes(&mut rung, &block_id);
        assert!(rung.nodes.iter().any(|node| {
            node.node_type == "variable" && node.data["block"]["handleId"] == "AUX"
        }));
        assert!(rung.nodes.iter().any(|node| {
            node.node_type == "variable" && node.data["block"]["handleId"] == "AUXQ"
        }));

        assert!(super::insert_ladder_block_handle_branch(
            &mut rung, &block_id, "AUX", "input", "contact",
        ));
        assert!(super::insert_ladder_block_handle_branch(
            &mut rung, &block_id, "AUXQ", "output", "coil",
        ));
        assert!(!rung.nodes.iter().any(|node| {
            node.node_type == "variable"
                && matches!(
                    node.data["block"]["handleId"].as_str(),
                    Some("AUX" | "AUXQ")
                )
        }));

        let input_branch_id = rung
            .nodes
            .iter()
            .find(|node| {
                node.node_type == "contact"
                    && rung.edges.iter().any(|edge| {
                        edge.source == node.id
                            && edge.target == block_id
                            && edge.target_handle == "AUX"
                    })
            })
            .unwrap()
            .id
            .clone();
        rung.nodes
            .iter_mut()
            .find(|node| node.id == input_branch_id)
            .unwrap()
            .data["variable"]["name"] = serde_json::Value::String("branch_enable".to_owned());

        let output_branch_id = rung
            .nodes
            .iter()
            .find(|node| {
                node.node_type == "coil"
                    && rung.edges.iter().any(|edge| {
                        edge.source == block_id
                            && edge.source_handle == "AUXQ"
                            && edge.target == node.id
                    })
            })
            .unwrap()
            .id
            .clone();
        rung.nodes
            .iter_mut()
            .find(|node| node.id == output_branch_id)
            .unwrap()
            .data["variable"]["name"] = serde_json::Value::String("branch_status".to_owned());

        let emitted = emit_ld_value(
            &to_value(LadderFlow {
                name: "main".to_owned(),
                rungs: vec![rung.clone()],
            })
            .unwrap(),
        )
        .unwrap();
        assert!(emitted.warnings.is_empty(), "{:?}", emitted.warnings);
        assert!(emitted.body_st.contains("gate0"), "{}", emitted.body_st);
        assert!(
            emitted.body_st.contains("branch_enable"),
            "{}",
            emitted.body_st
        );
        assert!(emitted.body_st.contains("AUX"), "{}", emitted.body_st);
        assert!(
            emitted.body_st.contains("branch_status"),
            "{}",
            emitted.body_st
        );
        assert!(emitted.body_st.contains("AUXQ"), "{}", emitted.body_st);

        assert!(remove_ladder_node(&mut rung, &input_branch_id));
        assert!(remove_ladder_node(&mut rung, &output_branch_id));
        assert!(rung.nodes.iter().any(|node| {
            node.node_type == "variable" && node.data["block"]["handleId"] == "AUX"
        }));
        assert!(rung.nodes.iter().any(|node| {
            node.node_type == "variable" && node.data["block"]["handleId"] == "AUXQ"
        }));
        for rail in rung
            .nodes
            .iter()
            .filter(|node| node.node_type == "powerRail")
        {
            assert!(!rail
                .data
                .get("handles")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .any(|handle| {
                    handle
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|id| id.starts_with("branch_"))
                }));
        }
    }

    #[test]
    fn ladder_parallel_branch_compiles_as_or_and_collapses_on_delete() {
        let mut rung = new_rung("main");
        assert!(insert_ladder_node(&mut rung, "contact", 144.0, 48.0));
        let anchor_id = rung
            .nodes
            .iter()
            .find(|node| node.node_type == "contact")
            .unwrap()
            .id
            .clone();
        rung.nodes
            .iter_mut()
            .find(|node| node.id == anchor_id)
            .unwrap()
            .data["variable"]["name"] = serde_json::Value::String("run_a".to_owned());

        assert!(insert_ladder_node(&mut rung, "contact", 160.0, 128.0));
        let branch_id = rung
            .nodes
            .iter()
            .find(|node| node.node_type == "contact" && node.id != anchor_id)
            .unwrap()
            .id
            .clone();
        rung.nodes
            .iter_mut()
            .find(|node| node.id == branch_id)
            .unwrap()
            .data["variable"]["name"] = serde_json::Value::String("run_b".to_owned());

        assert!(insert_ladder_node(&mut rung, "coil", 440.0, 48.0));
        rung.nodes
            .iter_mut()
            .find(|node| node.node_type == "coil")
            .unwrap()
            .data["variable"]["name"] = serde_json::Value::String("motor".to_owned());

        let body = to_value(LadderFlow {
            name: "main".to_owned(),
            rungs: vec![rung.clone()],
        })
        .unwrap();
        let emitted = emit_ld_value(&body).unwrap();
        assert!(emitted.warnings.is_empty(), "{:?}", emitted.warnings);
        assert!(emitted.body_st.contains("run_a"));
        assert!(emitted.body_st.contains("run_b"));
        assert!(emitted.body_st.contains(" OR "), "{}", emitted.body_st);
        assert!(emitted.body_st.contains("motor"));

        assert!(remove_ladder_node(&mut rung, &branch_id));
        assert_eq!(
            rung.nodes
                .iter()
                .filter(|node| node.node_type == "parallel")
                .count(),
            0
        );
        let collapsed = emit_ld_value(
            &to_value(LadderFlow {
                name: "main".to_owned(),
                rungs: vec![rung],
            })
            .unwrap(),
        )
        .unwrap();
        assert!(collapsed.warnings.is_empty(), "{:?}", collapsed.warnings);
        assert!(collapsed.body_st.contains("run_a"));
        assert!(!collapsed.body_st.contains("run_b"));
        assert!(!collapsed.body_st.contains(" OR "));
    }

    #[test]
    fn ladder_nested_parallel_drop_preserves_outer_branch_topology() {
        let mut rung = new_rung("main");
        assert!(insert_ladder_node(&mut rung, "contact", 144.0, 48.0));
        let run_a = rung
            .nodes
            .iter()
            .find(|node| node.node_type == "contact")
            .unwrap()
            .id
            .clone();
        rung.nodes
            .iter_mut()
            .find(|node| node.id == run_a)
            .unwrap()
            .data["variable"]["name"] = serde_json::Value::String("run_a".to_owned());

        assert!(insert_ladder_node(&mut rung, "contact", 160.0, 128.0));
        let run_b = rung
            .nodes
            .iter()
            .filter(|node| node.node_type == "contact" && node.id != run_a)
            .max_by(|left, right| left.position.y.total_cmp(&right.position.y))
            .unwrap()
            .id
            .clone();
        rung.nodes
            .iter_mut()
            .find(|node| node.id == run_b)
            .unwrap()
            .data["variable"]["name"] = serde_json::Value::String("run_b".to_owned());

        assert!(insert_ladder_node(&mut rung, "contact", 176.0, 208.0));
        let run_c = rung
            .nodes
            .iter()
            .filter(|node| node.node_type == "contact" && node.id != run_a && node.id != run_b)
            .max_by(|left, right| left.position.y.total_cmp(&right.position.y))
            .unwrap()
            .id
            .clone();
        rung.nodes
            .iter_mut()
            .find(|node| node.id == run_c)
            .unwrap()
            .data["variable"]["name"] = serde_json::Value::String("run_c".to_owned());

        assert_eq!(
            rung.nodes
                .iter()
                .filter(|node| node.node_type == "parallel")
                .count(),
            4
        );
        assert!(insert_ladder_node(&mut rung, "coil", 520.0, 48.0));
        rung.nodes
            .iter_mut()
            .find(|node| node.node_type == "coil")
            .unwrap()
            .data["variable"]["name"] = serde_json::Value::String("motor".to_owned());

        let emitted = emit_ld_value(
            &to_value(LadderFlow {
                name: "main".to_owned(),
                rungs: vec![rung.clone()],
            })
            .unwrap(),
        )
        .unwrap();
        assert!(emitted.warnings.is_empty(), "{:?}", emitted.warnings);
        for name in ["run_a", "run_b", "run_c", "motor"] {
            assert!(emitted.body_st.contains(name), "{}", emitted.body_st);
        }
        assert!(
            emitted.body_st.matches(" OR ").count() >= 2,
            "{}",
            emitted.body_st
        );

        assert!(remove_ladder_node(&mut rung, &run_c));
        assert_eq!(
            rung.nodes
                .iter()
                .filter(|node| node.node_type == "parallel")
                .count(),
            2
        );
        let collapsed_inner = emit_ld_value(
            &to_value(LadderFlow {
                name: "main".to_owned(),
                rungs: vec![rung.clone()],
            })
            .unwrap(),
        )
        .unwrap();
        assert!(
            collapsed_inner.warnings.is_empty(),
            "{:?}",
            collapsed_inner.warnings
        );
        assert!(collapsed_inner.body_st.contains("run_a"));
        assert!(collapsed_inner.body_st.contains("run_b"));
        assert!(!collapsed_inner.body_st.contains("run_c"));
        assert!(collapsed_inner.body_st.contains(" OR "));

        assert!(remove_ladder_node(&mut rung, &run_b));
        assert_eq!(
            rung.nodes
                .iter()
                .filter(|node| node.node_type == "parallel")
                .count(),
            0
        );
        let collapsed_outer = emit_ld_value(
            &to_value(LadderFlow {
                name: "main".to_owned(),
                rungs: vec![rung],
            })
            .unwrap(),
        )
        .unwrap();
        assert!(
            collapsed_outer.warnings.is_empty(),
            "{:?}",
            collapsed_outer.warnings
        );
        assert!(collapsed_outer.body_st.contains("run_a"));
        assert!(!collapsed_outer.body_st.contains("run_b"));
        assert!(!collapsed_outer.body_st.contains(" OR "));
    }

    #[test]
    fn ladder_delete_reconnects_a_serial_rung() {
        let mut rung = new_rung("main");
        let left = rung.nodes[0].id.clone();
        let right = rung.nodes[1].id.clone();
        assert!(insert_ladder_node(&mut rung, "contact", 144.0, 48.0));
        let contact = rung
            .nodes
            .iter()
            .find(|node| node.node_type == "contact")
            .unwrap()
            .id
            .clone();
        assert!(remove_ladder_node(&mut rung, &contact));
        assert_eq!(rung.nodes.len(), 2);
        assert_eq!(rung.edges.len(), 1);
        assert_eq!(rung.edges[0].source, left);
        assert_eq!(rung.edges[0].target, right);
        assert!(!remove_ladder_node(&mut rung, &left));
    }

    #[test]
    fn edited_ladder_nodes_compile_through_the_owned_graphical_walker() {
        let mut rung = new_rung("main");
        assert!(insert_ladder_node(&mut rung, "contact", 120.0, 48.0));
        assert!(insert_ladder_node(&mut rung, "coil", 280.0, 48.0));
        let contact = rung
            .nodes
            .iter_mut()
            .find(|node| node.node_type == "contact")
            .unwrap();
        contact.data["variable"]["name"] = serde_json::Value::String("run".to_owned());
        let coil = rung
            .nodes
            .iter_mut()
            .find(|node| node.node_type == "coil")
            .unwrap();
        coil.data["variable"]["name"] = serde_json::Value::String("motor".to_owned());

        let body = to_value(LadderFlow {
            name: "main".to_owned(),
            rungs: vec![rung],
        })
        .unwrap();
        let emitted = emit_ld_value(&body).unwrap();
        assert!(emitted.warnings.is_empty(), "{:?}", emitted.warnings);
        assert!(emitted.body_st.contains("motor"));
        assert!(emitted.body_st.contains("run"));
    }

    #[test]
    fn ladder_drop_rejects_unknown_toolbox_elements() {
        let mut rung = new_rung("main");
        let before = rung.clone();
        assert!(!insert_ladder_node(&mut rung, "comment", 64.0, 64.0));
        assert_eq!(rung, before);
    }
}
