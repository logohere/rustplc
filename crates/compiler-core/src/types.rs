use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TranspileProject {
    #[serde(default)]
    pub data_types: Vec<TranspileDataType>,
    #[serde(default)]
    pub pous: Vec<TranspilePou>,
    #[serde(default)]
    pub configuration: TranspileConfiguration,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TranspileConfiguration {
    #[serde(default)]
    pub tasks: Vec<TranspileTask>,
    #[serde(default)]
    pub instances: Vec<TranspileInstance>,
    #[serde(default)]
    pub global_variables: Vec<TranspileVariable>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TranspilePouKind {
    Program,
    Function,
    FunctionBlock,
}

impl TranspilePouKind {
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Program => "PROGRAM",
            Self::Function => "FUNCTION",
            Self::FunctionBlock => "FUNCTION_BLOCK",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranspileBodyLanguage {
    St,
    Il,
    Ld,
    Fbd,
    Sfc,
    Python,
    Cpp,
}

impl TranspileBodyLanguage {
    pub fn textual(self) -> bool {
        matches!(self, Self::St | Self::Il | Self::Python | Self::Cpp)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranspilePou {
    pub name: String,
    pub pou_type: TranspilePouKind,
    #[serde(default)]
    pub documentation: String,
    #[serde(default)]
    pub interface: TranspilePouInterface,
    pub body: TranspileBody,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TranspilePouInterface {
    #[serde(default)]
    pub return_type: Option<String>,
    #[serde(default)]
    pub variables: Vec<TranspileVariable>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranspileBody {
    pub language: TranspileBodyLanguage,
    pub value: Value,
}

impl TranspileBody {
    pub fn text(language: TranspileBodyLanguage, value: impl Into<String>) -> Self {
        Self {
            language,
            value: Value::String(value.into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TranspileVariableClass {
    Input,
    Output,
    InOut,
    External,
    Local,
    Temp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranspileVariable {
    pub name: String,
    pub variable_type: TranspileVariableType,
    #[serde(default)]
    pub class: Option<TranspileVariableClass>,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub initial_value: Option<String>,
    #[serde(default)]
    pub documentation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TranspileVariableType {
    BaseType {
        value: String,
    },
    Derived {
        value: String,
    },
    UserDataType {
        value: String,
    },
    Array {
        dimensions: Vec<String>,
        base_type: String,
    },
}

impl TranspileVariableType {
    pub fn declared_name(&self) -> String {
        match self {
            Self::BaseType { value } | Self::Derived { value } | Self::UserDataType { value } => {
                value.clone()
            }
            Self::Array {
                dimensions,
                base_type,
            } => format!(
                "ARRAY [{}] OF {}",
                dimensions.join(","),
                base_type.to_ascii_uppercase()
            ),
        }
    }

    pub fn derived_name(&self) -> Option<&str> {
        match self {
            Self::Derived { value } | Self::UserDataType { value } => Some(value),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TranspileDataType {
    Array {
        name: String,
        dimensions: Vec<String>,
        base_type: String,
        initial_value: Option<String>,
    },
    Enumerated {
        name: String,
        values: Vec<String>,
        initial_value: Option<String>,
    },
    Structure {
        name: String,
        variables: Vec<TranspileVariable>,
        initial_value: Option<String>,
    },
    DirectlyDerived {
        name: String,
        base_type: String,
        initial_value: Option<String>,
    },
}

impl TranspileDataType {
    pub fn name(&self) -> &str {
        match self {
            Self::Array { name, .. }
            | Self::Enumerated { name, .. }
            | Self::Structure { name, .. }
            | Self::DirectlyDerived { name, .. } => name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranspileTask {
    pub name: String,
    pub priority: i64,
    pub triggering: TranspileTaskTrigger,
    #[serde(default)]
    pub interval: Option<String>,
    #[serde(default)]
    pub single: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TranspileTaskTrigger {
    Cyclic,
    Interrupt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranspileInstance {
    pub name: String,
    pub program: String,
    #[serde(default)]
    pub task: Option<String>,
}
