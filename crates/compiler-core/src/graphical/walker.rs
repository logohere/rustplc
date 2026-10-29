use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use serde_json::Value;

use super::connection_types::{compute_connection_types, TypeContext};
use super::narrow::{
    block_data, coil_data, connector_name, contact_data, numeric_id, parallel_side,
    power_rail_side, variable_data, variable_expression, BlockData, BlockKind, CoilVariant,
    ContactVariant, PowerRailSide, VariableData, VariableVariant,
};
use super::path_tree::{chunks_text, compute_paths, factorize_paths, leaf, Chunk, PathNode};

#[derive(Debug, Clone, Deserialize)]
pub struct RfBody {
    #[serde(default)]
    pub rungs: Vec<RfRung>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RfFbdBody {
    pub rung: RfRung,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RfRung {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub nodes: Vec<RfNode>,
    #[serde(default)]
    pub edges: Vec<RfEdge>,
    #[serde(rename = "reactFlowViewport", default)]
    pub react_flow_viewport: Option<Vec<f64>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RfNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub position: RfPosition,
    #[serde(default)]
    pub data: Value,
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
pub struct RfPosition {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RfEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(rename = "sourceHandle", default)]
    pub source_handle: Option<String>,
    #[serde(rename = "targetHandle", default)]
    pub target_handle: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntheticVar {
    pub name: String,
    pub variable_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EmitResult {
    pub body_st: String,
    pub synthetic_vars: Vec<SyntheticVar>,
    pub warnings: Vec<String>,
}

struct WalkerState {
    by_id: HashMap<String, RfNode>,
    incoming: HashMap<String, Vec<RfEdge>>,
    outgoing: HashMap<String, Vec<RfEdge>>,
    program: Vec<Chunk>,
    current_indent: String,
    declared_vars: HashSet<String>,
    trigger_vars: Vec<SyntheticVar>,
    function_temp_vars: Vec<SyntheticVar>,
    emitted_blocks: HashSet<String>,
    consumers_by_block: HashMap<String, Vec<RfNode>>,
    emitted_sinks: HashSet<String>,
    connector_exprs: HashMap<String, Vec<Chunk>>,
    y_offset: HashMap<String, f64>,
    warnings: Vec<String>,
    conn_types: HashMap<String, String>,
}

pub fn emit_ld_value(value: &Value) -> Result<EmitResult, String> {
    let body: RfBody = serde_json::from_value(value.clone())
        .map_err(|error| format!("invalid LD body: {error}"))?;
    Ok(emit_ld_body(&body))
}

pub fn emit_ld_value_with_context(
    value: &Value,
    context: &TypeContext,
) -> Result<EmitResult, String> {
    let body: RfBody = serde_json::from_value(value.clone())
        .map_err(|error| format!("invalid LD body: {error}"))?;
    Ok(emit_ld_body_with_context(&body, context))
}

pub fn emit_fbd_value(value: &Value) -> Result<EmitResult, String> {
    let body: RfFbdBody = serde_json::from_value(value.clone())
        .map_err(|error| format!("invalid FBD body: {error}"))?;
    Ok(emit_ld_body(&RfBody {
        rungs: vec![body.rung],
    }))
}

pub fn emit_fbd_value_with_context(
    value: &Value,
    context: &TypeContext,
) -> Result<EmitResult, String> {
    let body: RfFbdBody = serde_json::from_value(value.clone())
        .map_err(|error| format!("invalid FBD body: {error}"))?;
    Ok(emit_ld_body_with_context(
        &RfBody {
            rungs: vec![body.rung],
        },
        context,
    ))
}

pub fn emit_ld_body(body: &RfBody) -> EmitResult {
    emit_ld_body_inner(body, None)
}

pub fn emit_ld_body_with_context(body: &RfBody, context: &TypeContext) -> EmitResult {
    emit_ld_body_inner(body, Some(context))
}

fn emit_ld_body_inner(body: &RfBody, context: Option<&TypeContext>) -> EmitResult {
    let mut state = WalkerState {
        by_id: HashMap::new(),
        incoming: HashMap::new(),
        outgoing: HashMap::new(),
        program: Vec::new(),
        current_indent: "  ".to_owned(),
        declared_vars: HashSet::new(),
        trigger_vars: Vec::new(),
        function_temp_vars: Vec::new(),
        emitted_blocks: HashSet::new(),
        consumers_by_block: HashMap::new(),
        emitted_sinks: HashSet::new(),
        connector_exprs: HashMap::new(),
        y_offset: HashMap::new(),
        warnings: Vec::new(),
        conn_types: context
            .map(|context| compute_connection_types(body, context))
            .unwrap_or_default(),
    };

    let mut cumulative_y = 0.0;
    for rung in &body.rungs {
        for node in &rung.nodes {
            if state.by_id.contains_key(&node.id) {
                state
                    .warnings
                    .push(format!("duplicate node id \"{}\" across rungs", node.id));
            }
            state.by_id.insert(node.id.clone(), node.clone());
            state.incoming.insert(node.id.clone(), Vec::new());
            state.outgoing.insert(node.id.clone(), Vec::new());
            state.y_offset.insert(node.id.clone(), cumulative_y);
        }
        for edge in &rung.edges {
            if let Some(bucket) = state.incoming.get_mut(&edge.target) {
                bucket.push(edge.clone());
            }
            if let Some(bucket) = state.outgoing.get_mut(&edge.source) {
                bucket.push(edge.clone());
            }
        }
        cumulative_y += rung_height(rung);
    }

    let mut sinks = Vec::new();
    for rung in &body.rungs {
        for node in &rung.nodes {
            if node.node_type == "coil" {
                if coil_data(&node.data).is_some_and(|data| !data.variable.is_empty()) {
                    sinks.push(node.clone());
                }
            } else if is_variable_node(node) {
                if let Some(data) = variable_data(&node.data) {
                    if !data.variable.is_empty()
                        && matches!(
                            data.variant,
                            VariableVariant::Output | VariableVariant::InOut
                        )
                    {
                        sinks.push(node.clone());
                    }
                }
            } else if node.node_type == "block" || node.node_type == "connector" {
                sinks.push(node.clone());
            }
        }
    }

    let mut ordered = Vec::new();
    let mut others = Vec::new();
    for node in sinks {
        if node_execution_order(&node) > 0 {
            ordered.push(node);
        } else {
            others.push(node);
        }
    }
    ordered.sort_by_key(node_execution_order);
    others.sort_by(|a, b| compare_position(&state, a, b));

    for node in ordered.iter().chain(others.iter()) {
        if let Some(block_id) = block_fed_sink(&state, node) {
            state
                .consumers_by_block
                .entry(block_id)
                .or_default()
                .push(node.clone());
        }
    }

    emit_sinks_with_coil_grouping(&mut state, &ordered);
    emit_sinks_with_coil_grouping(&mut state, &others);

    let mut body_st = String::from("\n");
    body_st.push_str(&chunks_text(&state.program));
    let mut synthetic_vars = state.trigger_vars;
    synthetic_vars.extend(state.function_temp_vars);
    EmitResult {
        body_st,
        synthetic_vars,
        warnings: state.warnings,
    }
}

fn rung_height(rung: &RfRung) -> f64 {
    rung.react_flow_viewport
        .as_ref()
        .and_then(|viewport| viewport.get(1))
        .copied()
        .filter(|value| value.is_finite())
        .unwrap_or_default()
}

fn is_variable_node(node: &RfNode) -> bool {
    matches!(
        node.node_type.as_str(),
        "variable" | "input-variable" | "output-variable" | "inout-variable"
    )
}

fn node_execution_order(node: &RfNode) -> i64 {
    if node.node_type == "coil" {
        return coil_data(&node.data)
            .map(|data| data.execution_order)
            .unwrap_or_default();
    }
    if node.node_type == "block" {
        return block_data(&node.data)
            .map(|data| data.execution_order)
            .unwrap_or_default();
    }
    if is_variable_node(node) {
        return variable_data(&node.data)
            .map(|data| data.execution_order)
            .unwrap_or_default();
    }
    0
}

fn compare_position(state: &WalkerState, a: &RfNode, b: &RfNode) -> std::cmp::Ordering {
    let ax = a.position.x.trunc() as i64;
    let ay = (a.position.y + state.y_offset.get(&a.id).copied().unwrap_or_default()).trunc() as i64;
    let bx = b.position.x.trunc() as i64;
    let by = (b.position.y + state.y_offset.get(&b.id).copied().unwrap_or_default()).trunc() as i64;
    if (ay - by).abs() >= 10 {
        ay.cmp(&by)
    } else {
        ax.cmp(&bx)
    }
}

fn block_fed_sink(state: &WalkerState, node: &RfNode) -> Option<String> {
    if node.node_type != "coil" && !is_variable_node(node) {
        return None;
    }
    let edges = state.incoming.get(&node.id)?;
    if edges.len() != 1 {
        return None;
    }
    let source = state.by_id.get(&edges[0].source)?;
    (source.node_type == "block").then(|| source.id.clone())
}

fn emit_sink(state: &mut WalkerState, node: &RfNode) {
    if node.node_type == "coil" {
        emit_coil_node(state, node);
    } else if is_variable_node(node) {
        if let Some(data) = variable_data(&node.data) {
            match data.variant {
                VariableVariant::Output => emit_out_variable(state, node, &data),
                VariableVariant::InOut => emit_inout_variable(state, node, &data),
                VariableVariant::Input => {}
            }
        }
    } else if node.node_type == "block" {
        emit_standalone_block(state, node);
    } else if node.node_type == "connector" {
        emit_connector(state, node);
    }
}

fn emit_sinks_with_coil_grouping(state: &mut WalkerState, sinks: &[RfNode]) {
    for index in 0..sinks.len() {
        if state.emitted_sinks.contains(&sinks[index].id) {
            continue;
        }
        let key = set_reset_group_key(state, &sinks[index]);
        let Some(key) = key else {
            state.emitted_sinks.insert(sinks[index].id.clone());
            emit_sink(state, &sinks[index]);
            continue;
        };
        let mut group = vec![sinks[index].clone()];
        for candidate in sinks.iter().skip(index + 1) {
            if state.emitted_sinks.contains(&candidate.id) {
                continue;
            }
            if set_reset_group_key(state, candidate).as_deref() == Some(key.as_str()) {
                group.push(candidate.clone());
            }
        }
        for node in &group {
            state.emitted_sinks.insert(node.id.clone());
        }
        if group.len() == 1 {
            emit_coil_node(state, &group[0]);
        } else {
            emit_coil_group(state, &group);
        }
    }
}

fn emit_block_consumers(state: &mut WalkerState, block_id: &str) {
    let Some(consumers) = state.consumers_by_block.get(block_id).cloned() else {
        return;
    };
    emit_sinks_with_coil_grouping(state, &consumers);
}

fn set_reset_group_key(state: &WalkerState, node: &RfNode) -> Option<String> {
    if node.node_type != "coil" {
        return None;
    }
    let data = coil_data(&node.data)?;
    if !matches!(data.variant, CoilVariant::Set | CoilVariant::Reset) {
        return None;
    }
    let mut sources = state
        .incoming
        .get(&node.id)?
        .iter()
        .map(|edge| edge.source.clone())
        .collect::<Vec<_>>();
    if sources.is_empty() {
        return None;
    }
    sources.sort();
    Some(sources.join("|"))
}

fn emit_coil_group(state: &mut WalkerState, coils: &[RfNode]) {
    let first = &coils[0];
    let paths = paths_from_incoming(state, &first.id, false);
    if paths.is_empty() {
        for coil in coils {
            emit_coil_node(state, coil);
        }
        return;
    }
    let expression = paths_to_chunks(&paths);
    state
        .program
        .push(Chunk::plain(format!("{}IF ", state.current_indent)));
    state.program.extend(expression);
    state.program.push(Chunk::plain(" THEN\n"));
    let inner = format!("{}  ", state.current_indent);
    for coil in coils {
        let Some(data) = coil_data(&coil.data) else {
            continue;
        };
        let (value, storage) = if data.variant == CoilVariant::Reset {
            ("FALSE", "reset")
        } else {
            ("TRUE", "set")
        };
        state.program.push(Chunk::plain(inner.clone()));
        state.program.push(Chunk::located(
            data.variable,
            loc(coil, "coil", "reference"),
        ));
        state
            .program
            .push(Chunk::plain(format!(" := {value}; (*{storage}*)\n")));
    }
    state
        .program
        .push(Chunk::plain(format!("{}END_IF;\n", state.current_indent)));
}

fn emit_coil_node(state: &mut WalkerState, node: &RfNode) {
    let Some(data) = coil_data(&node.data) else {
        state.warnings.push(format!(
            "coil node \"{}\" has unrecognised data shape",
            node.id
        ));
        return;
    };
    let paths = paths_from_incoming(state, &node.id, false);
    if paths.is_empty() {
        state
            .warnings
            .push(format!("Coil \"{}\" must be connected.", data.variable));
        return;
    }
    let expression = paths_to_chunks(&paths);
    let modified = apply_coil_modifier(state, data.variant, expression, node);
    state
        .program
        .push(Chunk::plain(state.current_indent.clone()));
    state.program.push(Chunk::located(
        data.variable,
        loc(node, "coil", "reference"),
    ));
    state.program.push(Chunk::plain(" := "));
    state.program.extend(modified);
    state.program.push(Chunk::plain(";\n"));
}

fn emit_out_variable(state: &mut WalkerState, node: &RfNode, data: &VariableData) {
    let paths = paths_from_incoming(state, &node.id, false);
    if paths.is_empty() {
        state.warnings.push(format!(
            "outVariable \"{}\" must be connected.",
            data.variable
        ));
        return;
    }
    let expression = paths_to_chunks(&paths);
    let eno = used_eno(state, &node.id);
    if let Some(eno) = &eno {
        state.program.push(Chunk::plain(format!(
            "{}IF {eno} THEN\n  ",
            state.current_indent
        )));
        state.current_indent.push_str("  ");
    }
    state
        .program
        .push(Chunk::plain(state.current_indent.clone()));
    state.program.push(Chunk::located(
        data.variable.clone(),
        loc(node, "io_variable", "expression"),
    ));
    state.program.push(Chunk::plain(" := "));
    state.program.extend(expression);
    state.program.push(Chunk::plain(";\n"));
    if eno.is_some() {
        state
            .current_indent
            .truncate(state.current_indent.len() - 2);
        state
            .program
            .push(Chunk::plain(format!("{}END_IF;\n", state.current_indent)));
    }
}

fn emit_inout_variable(state: &mut WalkerState, node: &RfNode, data: &VariableData) {
    let paths = paths_from_incoming(state, &node.id, false);
    if paths.is_empty() {
        state.warnings.push(format!(
            "inOutVariable \"{}\" must be connected.",
            data.variable
        ));
        return;
    }
    state
        .program
        .push(Chunk::plain(state.current_indent.clone()));
    state.program.push(Chunk::located(
        data.variable.clone(),
        loc(node, "io_variable", "expression"),
    ));
    state.program.push(Chunk::plain(" := "));
    state.program.extend(paths_to_chunks(&paths));
    state.program.push(Chunk::plain(";\n"));
}

fn used_eno(state: &WalkerState, node_id: &str) -> Option<String> {
    let edges = state.incoming.get(node_id)?;
    if edges.len() != 1 {
        return None;
    }
    let upstream = state.by_id.get(&edges[0].source)?;
    if upstream.node_type != "block" {
        return None;
    }
    let data = block_data(&upstream.data)?;
    let en_wired = state.incoming.get(&upstream.id).is_some_and(|edges| {
        edges
            .iter()
            .any(|edge| edge.target_handle.as_deref() == Some("EN"))
    });
    if !en_wired {
        return None;
    }
    Some(match data.block_kind {
        BlockKind::FunctionBlockInstance => format!("{}.ENO", data.instance_name),
        BlockKind::Function => format!("_TMP_{}{}_ENO", data.type_name, data.numeric_id),
    })
}

fn emit_connector(state: &mut WalkerState, node: &RfNode) {
    let Some(name) = connector_name(&node.data) else {
        state.warnings.push(format!(
            "connector node \"{}\" has unrecognised data shape",
            node.id
        ));
        return;
    };
    if state.connector_exprs.contains_key(&name) {
        return;
    }
    let paths = paths_from_incoming(state, &node.id, false);
    if !paths.is_empty() {
        state.connector_exprs.insert(name, paths_to_chunks(&paths));
    }
}

fn emit_standalone_block(state: &mut WalkerState, node: &RfNode) {
    let Some(data) = block_data(&node.data) else {
        state.warnings.push(format!(
            "block node \"{}\" has unrecognised data shape",
            node.id
        ));
        return;
    };
    match data.block_kind {
        BlockKind::FunctionBlockInstance => emit_function_block_call(state, node, &data),
        BlockKind::Function => emit_function_call(state, node, &data),
    }
}

fn paths_from_incoming(state: &mut WalkerState, node_id: &str, order: bool) -> Vec<PathNode> {
    let edges = state.incoming.get(node_id).cloned().unwrap_or_default();
    let mut paths = Vec::new();
    for edge in edges {
        let Some(upstream) = state.by_id.get(&edge.source).cloned() else {
            continue;
        };
        if let Some(path) = visit_upstream(state, &upstream, &edge, order) {
            paths.push(path);
        }
    }
    paths
}

fn visit_upstream(
    state: &mut WalkerState,
    node: &RfNode,
    edge: &RfEdge,
    order: bool,
) -> Option<PathNode> {
    match node.node_type.as_str() {
        "powerRail" => {
            (power_rail_side(&node.data) == Some(PowerRailSide::Left)).then_some(PathNode::True)
        }
        "contact" => Some(visit_contact(state, node, order)),
        "parallel" => visit_parallel(state, node, order),
        "variable" | "input-variable" | "output-variable" | "inout-variable" => {
            visit_variable(node)
        }
        "block" => visit_block_output(state, node, edge, order),
        "coil" => visit_coil_passthrough(state, node, order),
        "continuation" => visit_continuation(state, node),
        "connector" => None,
        other => {
            state
                .warnings
                .push(format!("unknown upstream node type \"{other}\""));
            None
        }
    }
}

fn visit_contact(state: &mut WalkerState, node: &RfNode, order: bool) -> PathNode {
    let Some(data) = contact_data(&node.data) else {
        state.warnings.push(format!(
            "contact node \"{}\" has unrecognised data shape",
            node.id
        ));
        return PathNode::True;
    };
    let variable = vec![Chunk::located(
        data.variable.clone(),
        loc(node, "contact", "reference"),
    )];
    let variable_leaf = leaf(apply_contact_modifier(state, data.variant, variable, node));
    let upstream = paths_from_incoming(state, &node.id, order);
    if upstream.is_empty() {
        state
            .warnings
            .push(format!("Contact \"{}\" must be connected.", data.variable));
        return variable_leaf;
    }
    if upstream.len() == 1 {
        return match &upstream[0] {
            PathNode::True => variable_leaf,
            PathNode::And(children) => {
                let mut combined = vec![variable_leaf];
                combined.extend(children.clone());
                PathNode::And(combined)
            }
            only => PathNode::And(vec![variable_leaf, only.clone()]),
        };
    }
    let factored = factorize_paths(&upstream);
    if factored.len() > 1 {
        return PathNode::And(vec![variable_leaf, PathNode::Or(factored)]);
    }
    let tail = factored.into_iter().next().unwrap_or(PathNode::True);
    let mut combined = vec![variable_leaf];
    match tail {
        PathNode::And(children) => combined.extend(children),
        other => combined.push(other),
    }
    PathNode::And(combined)
}

fn visit_coil_passthrough(state: &mut WalkerState, node: &RfNode, order: bool) -> Option<PathNode> {
    let upstream = paths_from_incoming(state, &node.id, order);
    match upstream.len() {
        0 => None,
        1 => Some(upstream[0].clone()),
        _ => {
            let factored = factorize_paths(&upstream);
            if factored.len() == 1 {
                Some(factored[0].clone())
            } else {
                Some(PathNode::Or(factored))
            }
        }
    }
}

fn visit_parallel(state: &mut WalkerState, node: &RfNode, order: bool) -> Option<PathNode> {
    let Some(side) = parallel_side(&node.data) else {
        state.warnings.push(format!(
            "parallel node \"{}\" has unrecognised data shape",
            node.id
        ));
        return None;
    };
    let paths = paths_from_incoming(state, &node.id, order);
    if paths.is_empty() {
        return None;
    }
    let mut flat = Vec::new();
    for path in paths {
        match path {
            PathNode::Or(children) => flat.extend(children),
            other => flat.push(other),
        }
    }
    if side == "open" && flat.len() == 1 {
        return flat.into_iter().next();
    }
    let factored = factorize_paths(&flat);
    if factored.len() == 1 {
        factored.into_iter().next()
    } else {
        Some(PathNode::Or(factored))
    }
}

fn visit_variable(node: &RfNode) -> Option<PathNode> {
    let expression = variable_expression(&node.data)
        .or_else(|| variable_data(&node.data).map(|data| data.variable))?;
    if expression.is_empty() {
        return None;
    }
    Some(leaf(vec![Chunk::located(
        expression,
        loc(node, "io_variable", "expression"),
    )]))
}

fn visit_continuation(state: &mut WalkerState, node: &RfNode) -> Option<PathNode> {
    let Some(name) = connector_name(&node.data) else {
        state.warnings.push(format!(
            "continuation node \"{}\" has unrecognised data shape",
            node.id
        ));
        return None;
    };
    if let Some(cached) = state.connector_exprs.get(&name).cloned() {
        return Some(leaf(cached));
    }
    let connector = state
        .by_id
        .values()
        .find(|candidate| {
            candidate.node_type == "connector"
                && connector_name(&candidate.data).as_deref() == Some(name.as_str())
        })
        .cloned();
    if let Some(connector) = connector {
        emit_connector(state, &connector);
        if let Some(cached) = state.connector_exprs.get(&name).cloned() {
            return Some(leaf(cached));
        }
    }
    state
        .warnings
        .push(format!("continuation \"{name}\" has no matching connector"));
    None
}

fn visit_block_output(
    state: &mut WalkerState,
    node: &RfNode,
    edge: &RfEdge,
    order: bool,
) -> Option<PathNode> {
    let data = block_data(&node.data)?;
    let requested = edge
        .source_handle
        .as_deref()
        .filter(|handle| !handle.is_empty())
        .map(str::to_owned)
        .or_else(|| data.outputs.first().cloned())
        .unwrap_or_else(|| "OUT".to_owned());
    match data.block_kind {
        BlockKind::FunctionBlockInstance => {
            if !order {
                emit_function_block_call(state, node, &data);
            }
            Some(leaf(vec![Chunk::located(
                format!("{}.{}", data.instance_name, requested),
                loc(node, "block", &format!("output/{requested}")),
            )]))
        }
        BlockKind::Function => {
            if !order {
                emit_function_call(state, node, &data);
            }
            Some(leaf(vec![Chunk::located(
                format!("_TMP_{}{}_{}", data.type_name, data.numeric_id, requested),
                loc(node, "block", &format!("output/{requested}")),
            )]))
        }
    }
}

fn emit_function_block_call(state: &mut WalkerState, node: &RfNode, data: &BlockData) {
    if !state.emitted_blocks.insert(node.id.clone()) {
        return;
    }
    let parts = build_input_args(state, node, data, true, data.execution_order > 0);
    state
        .program
        .push(Chunk::plain(state.current_indent.clone()));
    state.program.push(Chunk::located(
        data.instance_name.clone(),
        loc(node, "block", "instance"),
    ));
    state.program.push(Chunk::plain("("));
    push_arg_parts(&mut state.program, &parts);
    state.program.push(Chunk::plain(");\n"));
    emit_block_consumers(state, &node.id);
}

fn emit_function_call(state: &mut WalkerState, node: &RfNode, data: &BlockData) {
    if !state.emitted_blocks.insert(node.id.clone()) {
        return;
    }
    let wired_inputs = data
        .inputs
        .iter()
        .filter(|name| first_incoming_for_handle(state, &node.id, name).is_some())
        .count();
    let all_connected = data.extensible || wired_inputs == data.inputs.len();
    let use_named = data.outputs.len() > 1 || !all_connected;
    let mut parts = build_input_args(state, node, data, use_named, data.execution_order > 0);
    let mut primary = None;
    for (index, output) in data.outputs.iter().enumerate() {
        let temp_name = format!("_TMP_{}{}_{}", data.type_name, data.numeric_id, output);
        let temp_type = state
            .conn_types
            .get(&pin_out(&node.id, output))
            .cloned()
            .unwrap_or_else(|| {
                if output == "ENO" {
                    "BOOL".to_owned()
                } else {
                    "ANY".to_owned()
                }
            });
        state.function_temp_vars.push(SyntheticVar {
            name: temp_name.clone(),
            variable_type: temp_type,
        });
        let is_primary = data.outputs.len() == 1 || output.is_empty() || output == "OUT";
        if is_primary && primary.is_none() {
            primary = Some(temp_name);
        } else {
            parts.push(vec![
                Chunk::located(
                    output.clone(),
                    loc(node, "block", &format!("output/{index}")),
                ),
                Chunk::plain(format!(" => {temp_name}")),
            ]);
        }
    }
    let Some(primary) = primary else {
        return;
    };
    state
        .program
        .push(Chunk::plain(state.current_indent.clone()));
    state
        .program
        .push(Chunk::located(primary, loc(node, "block", "output/0")));
    state.program.push(Chunk::plain(" := "));
    state.program.push(Chunk::located(
        data.type_name.clone(),
        loc(node, "block", "type"),
    ));
    state.program.push(Chunk::plain("("));
    push_arg_parts(&mut state.program, &parts);
    state.program.push(Chunk::plain(");\n"));
    emit_block_consumers(state, &node.id);
}

fn build_input_args(
    state: &mut WalkerState,
    node: &RfNode,
    data: &BlockData,
    use_named: bool,
    order: bool,
) -> Vec<Vec<Chunk>> {
    let mut parts = Vec::new();
    for input in &data.inputs {
        if data.extensible {
            let edges = edges_for_input(state, &node.id, input);
            if edges.is_empty() {
                continue;
            }
            let edge = edges.last().expect("nonempty").clone();
            let Some(upstream) = state.by_id.get(&edge.source).cloned() else {
                continue;
            };
            let Some(path) = visit_upstream(state, &upstream, &edge, order) else {
                continue;
            };
            let expression = paths_to_chunks(&[path]);
            for _ in &edges {
                let mut part = Vec::new();
                if use_named {
                    part.push(Chunk::plain(format!("{input} := ")));
                }
                part.extend(expression.clone());
                parts.push(part);
            }
            continue;
        }
        let paths = paths_for_block_input(state, &node.id, input, order);
        if paths.is_empty() {
            continue;
        }
        let mut part = Vec::new();
        if use_named {
            part.push(Chunk::plain(format!("{input} := ")));
        }
        part.extend(paths_to_chunks(&paths));
        parts.push(part);
    }
    parts
}

fn edges_for_input(state: &WalkerState, block_id: &str, input: &str) -> Vec<RfEdge> {
    state
        .incoming
        .get(block_id)
        .into_iter()
        .flatten()
        .filter(|edge| edge.target_handle.as_deref() == Some(input))
        .cloned()
        .collect()
}

fn paths_for_block_input(
    state: &mut WalkerState,
    block_id: &str,
    input: &str,
    order: bool,
) -> Vec<PathNode> {
    let edges = edges_for_input(state, block_id, input);
    let mut paths = Vec::new();
    for edge in edges {
        let Some(upstream) = state.by_id.get(&edge.source).cloned() else {
            continue;
        };
        if let Some(path) = visit_upstream(state, &upstream, &edge, order) {
            paths.push(path);
        }
    }
    paths
}

fn first_incoming_for_handle(state: &WalkerState, block_id: &str, input: &str) -> Option<RfEdge> {
    state
        .incoming
        .get(block_id)?
        .iter()
        .find(|edge| edge.target_handle.as_deref() == Some(input))
        .cloned()
}

fn push_arg_parts(program: &mut Vec<Chunk>, parts: &[Vec<Chunk>]) {
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            program.push(Chunk::plain(", "));
        }
        program.extend(part.clone());
    }
}

fn paths_to_chunks(paths: &[PathNode]) -> Vec<Chunk> {
    if paths.is_empty() {
        return vec![Chunk::plain("TRUE")];
    }
    if paths.len() == 1 {
        return compute_paths(&paths[0], true);
    }
    let factored = factorize_paths(paths);
    if factored.len() == 1 {
        return compute_paths(&factored[0], true);
    }
    compute_paths(&PathNode::Or(factored), true)
}

fn apply_contact_modifier(
    state: &mut WalkerState,
    variant: ContactVariant,
    expression: Vec<Chunk>,
    node: &RfNode,
) -> Vec<Chunk> {
    match variant {
        ContactVariant::Default => expression,
        ContactVariant::Negated => {
            let mut out = vec![Chunk::located("NOT(", loc(node, "contact", "negated"))];
            out.extend(expression);
            out.push(Chunk::plain(")"));
            out
        }
        ContactVariant::RisingEdge => add_trigger(state, "R_TRIG", expression, node),
        ContactVariant::FallingEdge => add_trigger(state, "F_TRIG", expression, node),
    }
}

fn apply_coil_modifier(
    state: &mut WalkerState,
    variant: CoilVariant,
    expression: Vec<Chunk>,
    node: &RfNode,
) -> Vec<Chunk> {
    match variant {
        CoilVariant::Default => expression,
        CoilVariant::Negated => {
            let mut out = vec![Chunk::located("NOT(", loc(node, "coil", "negated"))];
            out.extend(expression);
            out.push(Chunk::plain(")"));
            out
        }
        CoilVariant::RisingEdge => add_trigger(state, "R_TRIG", expression, node),
        CoilVariant::FallingEdge => add_trigger(state, "F_TRIG", expression, node),
        CoilVariant::Set | CoilVariant::Reset => {
            let storage = if variant == CoilVariant::Set {
                "set"
            } else {
                "reset"
            };
            let value = if variant == CoilVariant::Set {
                "TRUE"
            } else {
                "FALSE"
            };
            state
                .program
                .push(Chunk::plain(format!("{}IF ", state.current_indent)));
            state.program.extend(expression);
            state.program.push(Chunk::plain(" THEN\n  "));
            vec![Chunk::plain(format!(
                "{value}; (*{storage}*)\n{}END_IF",
                state.current_indent
            ))]
        }
    }
}

fn add_trigger(
    state: &mut WalkerState,
    trigger_type: &str,
    expression: Vec<Chunk>,
    node: &RfNode,
) -> Vec<Chunk> {
    let mut index = 1;
    let mut name = format!("{trigger_type}{index}");
    while state.declared_vars.contains(&name) {
        index += 1;
        name = format!("{trigger_type}{index}");
    }
    state.declared_vars.insert(name.clone());
    state.trigger_vars.push(SyntheticVar {
        name: name.clone(),
        variable_type: trigger_type.to_owned(),
    });
    state
        .program
        .push(Chunk::plain(state.current_indent.clone()));
    state.program.push(Chunk::located(
        name.clone(),
        loc(node, "trigger", trigger_type),
    ));
    state.program.push(Chunk::plain("(CLK := "));
    state.program.extend(expression);
    state.program.push(Chunk::plain(");\n"));
    vec![Chunk::located(
        format!("{name}.Q"),
        loc(node, "trigger", trigger_type),
    )]
}

fn pin_out(node_id: &str, handle: &str) -> String {
    format!("out:{node_id}:{handle}")
}

fn loc(node: &RfNode, kind: &str, suffix: &str) -> String {
    format!(
        "P::main/{kind}/{}/{}",
        numeric_id(&node.data, &node.id),
        suffix
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn edge(
        id: usize,
        source: &str,
        target: &str,
        source_handle: Option<&str>,
        target_handle: Option<&str>,
    ) -> RfEdge {
        RfEdge {
            id: format!("e{id}"),
            source: source.to_owned(),
            target: target.to_owned(),
            source_handle: source_handle.map(str::to_owned),
            target_handle: target_handle.map(str::to_owned),
        }
    }

    fn node(id: &str, kind: &str, x: f64, y: f64, data: Value) -> RfNode {
        RfNode {
            id: id.to_owned(),
            node_type: kind.to_owned(),
            position: RfPosition { x, y },
            data,
        }
    }

    fn rail(id: &str, side: &str, x: f64) -> RfNode {
        node(id, "powerRail", x, 30.0, json!({"variant": side}))
    }

    fn input(id: &str, name: &str, x: f64, y: f64) -> RfNode {
        node(
            id,
            "variable",
            x,
            y,
            json!({
                "variant": "input", "variable": {"name": name}, "executionOrder": 0, "numericId": id
            }),
        )
    }

    fn output(id: &str, name: &str, x: f64, y: f64) -> RfNode {
        node(
            id,
            "variable",
            x,
            y,
            json!({
                "variant": "output", "variable": {"name": name}, "executionOrder": 0, "numericId": id
            }),
        )
    }

    fn coil(id: &str, name: &str, variant: &str, x: f64, y: f64, numeric: &str) -> RfNode {
        node(
            id,
            "coil",
            x,
            y,
            json!({
                "variant": variant, "variable": {"name": name}, "executionOrder": 0, "numericId": numeric
            }),
        )
    }

    fn contact(id: &str, name: &str, x: f64, numeric: &str) -> RfNode {
        node(
            id,
            "contact",
            x,
            38.0,
            json!({
                "variant": "default", "variable": {"name": name}, "numericId": numeric
            }),
        )
    }

    fn parallel(id: &str, side: &str, x: f64) -> RfNode {
        node(id, "parallel", x, 49.0, json!({"type": side}))
    }

    fn function(
        id: &str,
        name: &str,
        numeric: &str,
        order: i64,
        x: f64,
        y: f64,
        out_type: &str,
    ) -> RfNode {
        node(
            id,
            "block",
            x,
            y,
            json!({
                "numericId": numeric,
                "executionOrder": order,
                "executionControl": true,
                "variant": {
                    "name": name,
                    "type": "function",
                    "extensible": false,
                    "variables": [
                        {"name":"EN","class":"input","type":{"definition":"generic-type","value":"BOOL"}},
                        {"name":"ENO","class":"output","type":{"definition":"generic-type","value":"BOOL"}},
                        {"name":"OUT","class":"output","type":{"definition":"base-type","value":out_type}},
                        {"name":"IN1","class":"input","type":{"definition":"base-type","value":"DINT"}},
                        {"name":"IN2","class":"input","type":{"definition":"base-type","value":"DINT"}}
                    ]
                }
            }),
        )
    }

    #[test]
    fn issue_830_writes_output_before_downstream_call() {
        let nodes = vec![
            rail("L", "left", 0.0),
            input("901", "Input", 20.0, 80.0),
            input("902", "86400", 20.0, 120.0),
            function("MOD", "MOD", "6363443", 1, 150.0, 30.0, "DINT"),
            output("903", "Output", 320.0, 30.0),
            input("904", "Output", 20.0, 200.0),
            input("905", "60", 20.0, 240.0),
            function("DIV", "DIV", "4651235", 2, 450.0, 30.0, "DINT"),
            output("906", "Output", 620.0, 30.0),
            rail("R", "right", 760.0),
        ];
        let edges = vec![
            edge(0, "L", "MOD", Some("left-rail"), Some("EN")),
            edge(1, "901", "MOD", None, Some("IN1")),
            edge(2, "902", "MOD", None, Some("IN2")),
            edge(3, "MOD", "903", Some("OUT"), None),
            edge(4, "MOD", "DIV", Some("ENO"), Some("EN")),
            edge(5, "904", "DIV", None, Some("IN1")),
            edge(6, "905", "DIV", None, Some("IN2")),
            edge(7, "DIV", "906", Some("OUT"), None),
        ];
        let result = emit_ld_body(&RfBody {
            rungs: vec![RfRung {
                id: None,
                comment: String::new(),
                nodes,
                edges,
                react_flow_viewport: Some(vec![800.0, 300.0]),
            }],
        });
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert_eq!(
            result.body_st,
            "\n  _TMP_MOD6363443_OUT := MOD(EN := TRUE, IN1 := Input, IN2 := 86400, ENO => _TMP_MOD6363443_ENO);\n  IF _TMP_MOD6363443_ENO THEN\n      Output := _TMP_MOD6363443_OUT;\n  END_IF;\n  _TMP_DIV4651235_OUT := DIV(EN := _TMP_MOD6363443_ENO, IN1 := Output, IN2 := 60, ENO => _TMP_DIV4651235_ENO);\n  IF _TMP_DIV4651235_ENO THEN\n      Output := _TMP_DIV4651235_OUT;\n  END_IF;\n"
        );
    }

    #[test]
    fn issue_836_groups_parallel_set_coils() {
        let nodes = vec![
            rail("L", "left", 0.0),
            contact("C", "FirstScan", 68.0, "10"),
            parallel("PO", "open", 251.0),
            coil("K1", "Output1", "set", 300.0, 38.0, "20"),
            coil("K2", "Output2", "set", 300.0, 130.0, "21"),
            parallel("PC", "close", 373.0),
            coil("KR", "FirstScan", "reset", 500.0, 38.0, "22"),
            rail("R", "right", 600.0),
        ];
        let edges = vec![
            edge(0, "L", "C", None, None),
            edge(1, "C", "PO", None, None),
            edge(2, "PO", "K1", None, None),
            edge(3, "PO", "K2", None, None),
            edge(4, "K1", "PC", None, None),
            edge(5, "K2", "PC", None, None),
            edge(6, "PC", "KR", None, None),
            edge(7, "KR", "R", None, None),
        ];
        let result = emit_ld_body(&RfBody {
            rungs: vec![RfRung {
                id: None,
                comment: String::new(),
                nodes,
                edges,
                react_flow_viewport: Some(vec![700.0, 300.0]),
            }],
        });
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert_eq!(
            result.body_st,
            "\n  IF FirstScan THEN\n    Output1 := TRUE; (*set*)\n    Output2 := TRUE; (*set*)\n  END_IF;\n  IF FirstScan THEN\n    FirstScan := FALSE; (*reset*)\n  END_IF;\n"
        );
    }
}
