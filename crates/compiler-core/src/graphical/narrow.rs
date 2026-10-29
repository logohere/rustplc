use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactVariant {
    Default,
    Negated,
    RisingEdge,
    FallingEdge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoilVariant {
    Default,
    Negated,
    RisingEdge,
    FallingEdge,
    Set,
    Reset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerRailSide {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableVariant {
    Input,
    Output,
    InOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Function,
    FunctionBlockInstance,
}

#[derive(Debug, Clone)]
pub struct ContactData {
    pub variant: ContactVariant,
    pub variable: String,
}

#[derive(Debug, Clone)]
pub struct CoilData {
    pub variant: CoilVariant,
    pub variable: String,
    pub execution_order: i64,
}

#[derive(Debug, Clone)]
pub struct VariableData {
    pub variant: VariableVariant,
    pub variable: String,
    pub execution_order: i64,
}

#[derive(Debug, Clone)]
pub struct BlockData {
    pub type_name: String,
    pub block_kind: BlockKind,
    pub instance_name: String,
    pub numeric_id: String,
    pub execution_order: i64,
    pub execution_control: bool,
    pub extensible: bool,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

pub fn variable_name(data: &Value) -> String {
    data.get("variable")
        .and_then(Value::as_object)
        .and_then(|variable| variable.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

pub fn contact_data(data: &Value) -> Option<ContactData> {
    let variant = match data.get("variant")?.as_str()? {
        "default" => ContactVariant::Default,
        "negated" => ContactVariant::Negated,
        "risingEdge" => ContactVariant::RisingEdge,
        "fallingEdge" => ContactVariant::FallingEdge,
        _ => return None,
    };
    Some(ContactData {
        variant,
        variable: variable_name(data),
    })
}

pub fn coil_data(data: &Value) -> Option<CoilData> {
    let variant = match data.get("variant")?.as_str()? {
        "default" => CoilVariant::Default,
        "negated" => CoilVariant::Negated,
        "risingEdge" => CoilVariant::RisingEdge,
        "fallingEdge" => CoilVariant::FallingEdge,
        "set" => CoilVariant::Set,
        "reset" => CoilVariant::Reset,
        _ => return None,
    };
    Some(CoilData {
        variant,
        variable: variable_name(data),
        execution_order: data
            .get("executionOrder")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
    })
}

pub fn power_rail_side(data: &Value) -> Option<PowerRailSide> {
    match data.get("variant")?.as_str()? {
        "left" => Some(PowerRailSide::Left),
        "right" => Some(PowerRailSide::Right),
        _ => None,
    }
}

pub fn variable_data(data: &Value) -> Option<VariableData> {
    let variant = match data.get("variant")?.as_str()? {
        "input" | "input-variable" => VariableVariant::Input,
        "output" | "output-variable" => VariableVariant::Output,
        "inout" | "inOut" | "inout-variable" => VariableVariant::InOut,
        _ => return None,
    };
    Some(VariableData {
        variant,
        variable: variable_name(data),
        execution_order: data
            .get("executionOrder")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
    })
}

pub fn variable_expression(data: &Value) -> Option<String> {
    data.get("expression")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

pub fn parallel_side(data: &Value) -> Option<&str> {
    match data.get("type")?.as_str()? {
        "open" => Some("open"),
        "close" => Some("close"),
        _ => None,
    }
}

pub fn connector_name(data: &Value) -> Option<String> {
    data.get("name").and_then(Value::as_str).map(str::to_owned)
}

pub fn block_data(data: &Value) -> Option<BlockData> {
    let variant = data.get("variant")?.as_object()?;
    let type_name = variant.get("name")?.as_str()?.to_owned();
    let block_kind = match variant.get("type").and_then(Value::as_str) {
        Some("function-block") | Some("function-block-instance") => {
            BlockKind::FunctionBlockInstance
        }
        _ => BlockKind::Function,
    };
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    if let Some(variables) = variant.get("variables").and_then(Value::as_array) {
        for variable in variables {
            let Some(variable) = variable.as_object() else {
                continue;
            };
            let Some(name) = variable.get("name").and_then(Value::as_str) else {
                continue;
            };
            match variable.get("class").and_then(Value::as_str) {
                Some("input") => inputs.push(name.to_owned()),
                Some("output") => outputs.push(name.to_owned()),
                Some("inOut") | Some("inout") => inputs.push(name.to_owned()),
                _ => {}
            }
        }
    }
    let numeric_id = match data.get("numericId") {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        _ => String::new(),
    };
    Some(BlockData {
        type_name,
        block_kind,
        instance_name: variable_name(data),
        numeric_id,
        execution_order: data
            .get("executionOrder")
            .and_then(Value::as_i64)
            .unwrap_or_default(),
        execution_control: data
            .get("executionControl")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        extensible: variant
            .get("extensible")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        inputs,
        outputs,
    })
}

pub fn numeric_id(data: &Value, fallback: &str) -> String {
    match data.get("numericId") {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        _ => fallback.to_owned(),
    }
}
