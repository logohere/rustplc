use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VariableClass {
    #[serde(rename = "input")]
    Input,
    #[serde(rename = "output")]
    Output,
    #[serde(rename = "inOut")]
    InOut,
    #[serde(rename = "external")]
    External,
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "temp")]
    Temp,
    #[serde(rename = "global")]
    Global,
}

impl VariableClass {
    pub fn openplc_name(self) -> &'static str {
        match self {
            Self::Input => "INPUT",
            Self::Output => "OUTPUT",
            Self::InOut => "INOUT",
            Self::External => "EXTERNAL",
            Self::Local => "LOCAL",
            Self::Temp => "TEMP",
            Self::Global => "GLOBAL",
        }
    }

    pub fn allows_location(self) -> bool {
        matches!(self, Self::Local | Self::Global)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VariableTypeDefinition {
    BaseType,
    UserDataType,
    Array,
    Derived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VariableTypeBase {
    pub definition: VariableTypeDefinition,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrayDimension {
    pub dimension: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArrayTypeData {
    #[serde(rename = "baseType")]
    pub base_type: VariableTypeBase,
    pub dimensions: Vec<ArrayDimension>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlcVariableType {
    pub definition: VariableTypeDefinition,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ArrayTypeData>,
}

impl PlcVariableType {
    pub fn base(value: impl Into<String>) -> Self {
        Self {
            definition: VariableTypeDefinition::BaseType,
            value: value.into(),
            data: None,
        }
    }

    pub fn user(value: impl Into<String>) -> Self {
        Self {
            definition: VariableTypeDefinition::UserDataType,
            value: value.into(),
            data: None,
        }
    }

    pub fn derived(value: impl Into<String>) -> Self {
        Self {
            definition: VariableTypeDefinition::Derived,
            value: value.into(),
            data: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlcVariable {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<VariableClass>,
    #[serde(rename = "type")]
    pub variable_type: PlcVariableType,
    pub location: String,
    #[serde(rename = "initialValue", skip_serializing_if = "Option::is_none")]
    pub initial_value: Option<String>,
    pub documentation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PouType {
    Program,
    Function,
    FunctionBlock,
}

impl PouType {
    pub fn start_keyword(self) -> &'static str {
        match self {
            Self::Program => "PROGRAM",
            Self::Function => "FUNCTION",
            Self::FunctionBlock => "FUNCTION_BLOCK",
        }
    }

    pub fn end_keyword(self) -> &'static str {
        match self {
            Self::Program => "END_PROGRAM",
            Self::Function => "END_FUNCTION",
            Self::FunctionBlock => "END_FUNCTION_BLOCK",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PouLanguage {
    St,
    Il,
    Ld,
    Fbd,
    Python,
    Cpp,
}

impl PouLanguage {
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str()
        {
            "st" => Some(Self::St),
            "il" => Some(Self::Il),
            "ld" => Some(Self::Ld),
            "fbd" => Some(Self::Fbd),
            "py" => Some(Self::Python),
            "cpp" => Some(Self::Cpp),
            _ => None,
        }
    }

    pub fn is_textual(self) -> bool {
        matches!(self, Self::St | Self::Il)
    }

    pub fn is_graphical(self) -> bool {
        matches!(self, Self::Ld | Self::Fbd)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PouInterface {
    #[serde(rename = "returnType", skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    pub variables: Vec<PlcVariable>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PouBody {
    pub language: PouLanguage,
    pub value: Value,
}

impl PouBody {
    pub fn text(language: PouLanguage, value: impl Into<String>) -> Self {
        Self {
            language,
            value: Value::String(value.into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct GraphPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GraphMeasured {
    pub width: f64,
    pub height: f64,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub position: GraphPosition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub measured: Option<GraphMeasured>,
    #[serde(default = "default_true")]
    pub draggable: bool,
    #[serde(default = "default_true")]
    pub selectable: bool,
    #[serde(default)]
    pub data: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    #[serde(rename = "sourceHandle", default)]
    pub source_handle: String,
    pub target: String,
    #[serde(rename = "targetHandle", default)]
    pub target_handle: String,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub edge_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LadderRung {
    pub id: String,
    #[serde(default)]
    pub comment: String,
    #[serde(rename = "defaultBounds", default)]
    pub default_bounds: Vec<f64>,
    #[serde(rename = "reactFlowViewport", default)]
    pub react_flow_viewport: Vec<f64>,
    #[serde(default)]
    pub nodes: Vec<GraphNode>,
    #[serde(default)]
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LadderFlow {
    pub name: String,
    #[serde(default)]
    pub rungs: Vec<LadderRung>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FbdRung {
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub nodes: Vec<GraphNode>,
    #[serde(default)]
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GraphViewport {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

impl Default for GraphViewport {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FbdFlow {
    pub name: String,
    #[serde(default)]
    pub viewport: GraphViewport,
    pub rung: FbdRung,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedPou {
    pub name: String,
    #[serde(rename = "pouType")]
    pub pou_type: PouType,
    pub interface: PouInterface,
    pub body: PouBody,
    pub documentation: String,
}
