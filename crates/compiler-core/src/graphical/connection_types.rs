use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::narrow::{block_data, connector_name, variable_data, variable_expression, BlockData};
use super::walker::{RfBody, RfEdge, RfNode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockIo {
    pub name: String,
    pub variable_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlockInfos {
    pub name: String,
    pub function_block: bool,
    pub extensible: bool,
    pub inputs: Vec<BlockIo>,
    pub outputs: Vec<BlockIo>,
}

#[derive(Debug, Clone, Default)]
pub struct TypeContext {
    pub variable_types: HashMap<String, String>,
    pub block_infos: HashMap<String, BlockInfos>,
    pub structure_fields: HashMap<String, HashMap<String, String>>,
    pub function_name: Option<String>,
    pub function_return_type: Option<String>,
}

impl TypeContext {
    pub fn variable_type(&self, expression: &str) -> Option<String> {
        let mut parts = expression.split('.');
        let first = parts.next().unwrap_or_default();
        let mut current = if self.function_name.as_deref() == Some(first) {
            self.function_return_type.clone()
        } else {
            self.variable_types.get(first).cloned()
        }?;
        for part in parts {
            if let Some(block) = self.block_infos.get(&current) {
                current = block
                    .inputs
                    .iter()
                    .chain(block.outputs.iter())
                    .find(|io| io.name == part)
                    .map(|io| io.variable_type.clone())?;
                continue;
            }
            if let Some(fields) = self.structure_fields.get(&current) {
                current = fields.get(part)?.clone();
                continue;
            }
            return None;
        }
        Some(current)
    }

    pub fn resolve_block(&self, name: &str) -> Option<BlockInfos> {
        self.block_infos.get(name).cloned()
    }
}

pub fn pin_in(node_id: &str, handle: &str) -> String {
    format!("in:{node_id}:{handle}")
}

pub fn pin_out(node_id: &str, handle: &str) -> String {
    format!("out:{node_id}:{handle}")
}

pub fn literal_type(expression: &str) -> Option<String> {
    if let Some((prefix, _)) = expression.split_once('#') {
        let upper = prefix.to_ascii_uppercase();
        return match upper.as_str() {
            "T" => Some("TIME".to_owned()),
            "D" => Some("DATE".to_owned()),
            "TOD" => Some("TIME_OF_DAY".to_owned()),
            "DT" => Some("DATE_AND_TIME".to_owned()),
            "2" | "8" | "16" => None,
            _ => Some(upper),
        };
    }
    if expression.starts_with('\'') {
        return Some("STRING".to_owned());
    }
    if expression.starts_with('"') {
        return Some("WSTRING".to_owned());
    }
    None
}

struct Graph {
    by_id: HashMap<String, RfNode>,
    incoming: HashMap<String, Vec<RfEdge>>,
}

#[derive(Default)]
struct PinTypes {
    types: HashMap<String, String>,
    related: Vec<Vec<String>>,
}

impl PinTypes {
    fn extract_related(&mut self, pin: &str) -> Vec<String> {
        if let Some(index) = self
            .related
            .iter()
            .position(|group| group.iter().any(|candidate| candidate == pin))
        {
            return self.related.remove(index);
        }
        vec![pin.to_owned()]
    }

    fn assign(&mut self, pins: impl IntoIterator<Item = String>, variable_type: &str) {
        for pin in pins {
            self.types.insert(pin, variable_type.to_owned());
        }
    }
}

pub fn compute_connection_types(body: &RfBody, context: &TypeContext) -> HashMap<String, String> {
    let mut graph = Graph {
        by_id: HashMap::new(),
        incoming: HashMap::new(),
    };
    let mut nodes = Vec::new();
    for rung in &body.rungs {
        for node in &rung.nodes {
            graph.by_id.insert(node.id.clone(), node.clone());
            graph.incoming.insert(node.id.clone(), Vec::new());
            nodes.push(node.clone());
        }
        for edge in &rung.edges {
            if let Some(bucket) = graph.incoming.get_mut(&edge.target) {
                bucket.push(edge.clone());
            }
        }
    }

    let mut pin_types = PinTypes::default();
    let mut deferred = Vec::new();

    for node in &nodes {
        match node.node_type.as_str() {
            "variable" | "input-variable" | "output-variable" | "inout-variable" => {
                let Some(data) = variable_data(&node.data) else {
                    continue;
                };
                let expression = variable_expression(&node.data).unwrap_or(data.variable.clone());
                let Some(variable_type) = context
                    .variable_type(&expression)
                    .or_else(|| literal_type(&expression))
                else {
                    continue;
                };
                if matches!(
                    data.variant,
                    super::narrow::VariableVariant::Input | super::narrow::VariableVariant::InOut
                ) {
                    let related = pin_types.extract_related(&pin_out(&node.id, ""));
                    pin_types.assign(related, &variable_type);
                }
                if matches!(
                    data.variant,
                    super::narrow::VariableVariant::Output | super::narrow::VariableVariant::InOut
                ) {
                    pin_types
                        .types
                        .insert(pin_in(&node.id, ""), variable_type.clone());
                    if let Some(source) = source_pin(&graph, &node.id, None) {
                        if !pin_types.types.contains_key(&source) {
                            let related = pin_types.extract_related(&source);
                            pin_types.assign(related, &variable_type);
                        }
                    }
                }
            }
            "contact" | "coil" | "parallel" | "powerRail" => {
                let related = pin_types.extract_related(&pin_out(&node.id, ""));
                pin_types.assign(related, "BOOL");
                pin_types
                    .types
                    .insert(pin_in(&node.id, ""), "BOOL".to_owned());
                for edge in graph.incoming.get(&node.id).cloned().unwrap_or_default() {
                    let source = edge_source_pin(&graph, &edge);
                    if !pin_types.types.contains_key(&source) {
                        let related = pin_types.extract_related(&source);
                        pin_types.assign(related, "BOOL");
                    }
                }
            }
            "continuation" => {
                let Some(name) = connector_name(&node.data) else {
                    continue;
                };
                let connector = nodes.iter().find(|candidate| {
                    candidate.node_type == "connector"
                        && connector_name(&candidate.data).as_deref() == Some(name.as_str())
                });
                let Some(connector) = connector else {
                    continue;
                };
                let mut undefined = vec![pin_out(&node.id, ""), pin_in(&connector.id, "")];
                if let Some(source) = source_pin(&graph, &connector.id, None) {
                    undefined.push(source);
                }
                let mut variable_type = "ANY".to_owned();
                let mut related = Vec::new();
                for pin in undefined {
                    if let Some(known) = pin_types.types.get(&pin) {
                        variable_type = known.clone();
                    } else {
                        related.extend(pin_types.extract_related(&pin));
                    }
                }
                if variable_type.starts_with("ANY") && !related.is_empty() {
                    pin_types.related.push(dedup(related));
                } else {
                    pin_types.assign(related, &variable_type);
                }
            }
            "block" => {
                let Some(data) = block_data(&node.data) else {
                    continue;
                };
                let infos = block_infos_from_variant(&node.data)
                    .or_else(|| context.resolve_block(&data.type_name));
                if let Some(infos) = infos {
                    compute_block_input_types(&graph, &mut pin_types, node, &data, &infos);
                } else {
                    for handle in wired_input_handles(&graph, node, &data) {
                        let input_pin = pin_in(&node.id, &handle);
                        let Some(source) = source_pin(&graph, &node.id, Some(&handle)) else {
                            continue;
                        };
                        if let Some(source_type) = pin_types.types.get(&source).cloned() {
                            pin_types.types.insert(input_pin, source_type);
                        } else {
                            let mut related = pin_types.extract_related(&source);
                            related.push(input_pin);
                            pin_types.related.push(dedup(related));
                        }
                    }
                    deferred.push((node.clone(), data));
                }
            }
            _ => {}
        }
    }

    for (node, data) in deferred {
        let infos = synthesize_permissive(&graph, &node, &data);
        compute_block_input_types(&graph, &mut pin_types, &node, &data, &infos);
    }
    pin_types.types
}

fn compute_block_input_types(
    graph: &Graph,
    pin_types: &mut PinTypes,
    node: &RfNode,
    data: &BlockData,
    infos: &BlockInfos,
) {
    let mut undefined_groups: HashMap<String, Vec<String>> = HashMap::new();
    let mut bucket = |any_class: &str, pin: String| {
        undefined_groups
            .entry(any_class.to_owned())
            .or_default()
            .push(pin);
    };

    for output in output_handles(data) {
        let pin = pin_out(&node.id, &output);
        if output == "ENO" {
            let related = pin_types.extract_related(&pin);
            pin_types.assign(related, "BOOL");
            continue;
        }
        for formal in &infos.outputs {
            if formal.name != output {
                continue;
            }
            if formal.variable_type.starts_with("ANY") {
                bucket(&formal.variable_type, pin.clone());
            } else if !pin_types.types.contains_key(&pin) {
                let related = pin_types.extract_related(&pin);
                pin_types.assign(related, &formal.variable_type);
            }
        }
    }

    for handle in wired_input_handles(graph, node, data) {
        let pin = pin_in(&node.id, &handle);
        if handle == "EN" {
            let related = pin_types.extract_related(&pin);
            pin_types.assign(related, "BOOL");
            continue;
        }
        for formal in &infos.inputs {
            if formal.name != handle {
                continue;
            }
            let source = source_pin(graph, &node.id, Some(&handle));
            if formal.variable_type.starts_with("ANY") {
                bucket(&formal.variable_type, pin.clone());
                if let Some(source) = source {
                    bucket(&formal.variable_type, source);
                }
            } else {
                pin_types
                    .types
                    .insert(pin.clone(), formal.variable_type.clone());
                if let Some(source) = source {
                    if !pin_types.types.contains_key(&source) {
                        let related = pin_types.extract_related(&source);
                        pin_types.assign(related, &formal.variable_type);
                    }
                }
            }
        }
    }

    for (any_class, pins) in undefined_groups {
        let mut variable_type = any_class;
        let mut related = Vec::new();
        for pin in pins {
            if let Some(known) = pin_types.types.get(&pin) {
                if !known.starts_with("ANY") {
                    variable_type = known.clone();
                }
            } else {
                related.extend(pin_types.extract_related(&pin));
            }
        }
        related = dedup(related);
        if variable_type.starts_with("ANY") && !related.is_empty() {
            pin_types.related.push(related);
        } else {
            pin_types.assign(related, &variable_type);
        }
    }
}

fn block_infos_from_variant(data: &Value) -> Option<BlockInfos> {
    let variant = data.get("variant")?.as_object()?;
    let name = variant.get("name")?.as_str()?.to_owned();
    let function_block = matches!(
        variant.get("type").and_then(Value::as_str),
        Some("function-block") | Some("function-block-instance")
    );
    let extensible = variant
        .get("extensible")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for variable in variant
        .get("variables")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(variable) = variable.as_object() else {
            continue;
        };
        let Some(variable_name) = variable.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(variable_type) = variable
            .get("type")
            .and_then(|value| value.get("value"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let io = BlockIo {
            name: variable_name.to_owned(),
            variable_type: normalize_type_name(variable_type),
        };
        match variable.get("class").and_then(Value::as_str) {
            Some("input") => inputs.push(io),
            Some("output") => outputs.push(io),
            Some("inOut") | Some("inout") => {
                inputs.push(io.clone());
                outputs.push(io);
            }
            _ => {}
        }
    }
    Some(BlockInfos {
        name,
        function_block,
        extensible,
        inputs,
        outputs,
    })
}

fn synthesize_permissive(graph: &Graph, node: &RfNode, data: &BlockData) -> BlockInfos {
    BlockInfos {
        name: data.type_name.clone(),
        function_block: matches!(
            data.block_kind,
            super::narrow::BlockKind::FunctionBlockInstance
        ),
        extensible: false,
        inputs: wired_input_handles(graph, node, data)
            .into_iter()
            .filter(|handle| handle != "EN")
            .map(|name| BlockIo {
                name,
                variable_type: "ANY".to_owned(),
            })
            .collect(),
        outputs: output_handles(data)
            .into_iter()
            .filter(|handle| handle != "ENO")
            .map(|name| BlockIo {
                name,
                variable_type: "ANY".to_owned(),
            })
            .collect(),
    }
}

fn wired_input_handles(graph: &Graph, node: &RfNode, data: &BlockData) -> Vec<String> {
    let wired = graph
        .incoming
        .get(&node.id)
        .into_iter()
        .flatten()
        .map(|edge| edge.target_handle.clone().unwrap_or_default())
        .collect::<HashSet<_>>();
    let mut handles = Vec::new();
    if wired.contains("EN") {
        handles.push("EN".to_owned());
    }
    for name in &data.inputs {
        if name != "EN" && wired.contains(name) {
            handles.push(name.clone());
        }
    }
    handles
}

fn output_handles(data: &BlockData) -> Vec<String> {
    let mut handles = data.outputs.clone();
    if data.execution_control && !handles.iter().any(|handle| handle == "ENO") {
        handles.push("ENO".to_owned());
    }
    handles
}

fn source_pin(graph: &Graph, node_id: &str, handle: Option<&str>) -> Option<String> {
    graph.incoming.get(node_id)?.iter().find_map(|edge| {
        if handle.is_none_or(|handle| edge.target_handle.as_deref() == Some(handle)) {
            Some(edge_source_pin(graph, edge))
        } else {
            None
        }
    })
}

fn edge_source_pin(graph: &Graph, edge: &RfEdge) -> String {
    if graph
        .by_id
        .get(&edge.source)
        .is_some_and(|node| node.node_type == "block")
    {
        pin_out(
            &edge.source,
            edge.source_handle.as_deref().unwrap_or_default(),
        )
    } else {
        pin_out(&edge.source, "")
    }
}

fn normalize_type_name(value: &str) -> String {
    let upper = value.to_ascii_uppercase();
    if upper.starts_with("ANY")
        || matches!(
            upper.as_str(),
            "BOOL"
                | "BYTE"
                | "WORD"
                | "DWORD"
                | "LWORD"
                | "SINT"
                | "INT"
                | "DINT"
                | "LINT"
                | "USINT"
                | "UINT"
                | "UDINT"
                | "ULINT"
                | "REAL"
                | "LREAL"
                | "TIME"
                | "DATE"
                | "TOD"
                | "DT"
                | "STRING"
                | "WSTRING"
                | "__XWORD"
        )
    {
        upper
    } else {
        value.to_owned()
    }
}

fn dedup(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphical::walker::{RfPosition, RfRung};
    use serde_json::json;

    #[test]
    fn infers_generic_function_output_from_dint_inputs() {
        let input = RfNode {
            id: "input".to_owned(),
            node_type: "variable".to_owned(),
            position: RfPosition::default(),
            data: json!({"variant":"input","variable":{"name":"x"}}),
        };
        let add = RfNode {
            id: "add".to_owned(),
            node_type: "block".to_owned(),
            position: RfPosition::default(),
            data: json!({
                "numericId":"10",
                "executionControl":false,
                "variant":{
                    "name":"ADD","type":"function","variables":[
                        {"name":"OUT","class":"output","type":{"definition":"generic-type","value":"ANY_NUM"}},
                        {"name":"IN1","class":"input","type":{"definition":"generic-type","value":"ANY_NUM"}}
                    ]
                }
            }),
        };
        let body = RfBody {
            rungs: vec![RfRung {
                id: None,
                comment: String::new(),
                nodes: vec![input, add],
                edges: vec![RfEdge {
                    id: "e".to_owned(),
                    source: "input".to_owned(),
                    target: "add".to_owned(),
                    source_handle: None,
                    target_handle: Some("IN1".to_owned()),
                }],
                react_flow_viewport: None,
            }],
        };
        let mut context = TypeContext::default();
        context
            .variable_types
            .insert("x".to_owned(), "DINT".to_owned());
        let types = compute_connection_types(&body, &context);
        assert_eq!(
            types.get(&pin_out("add", "OUT")).map(String::as_str),
            Some("DINT")
        );
    }
}
