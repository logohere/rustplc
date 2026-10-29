use std::collections::BTreeMap;

use crate::plc_model::{ParsedPou, PlcVariable, PouLanguage, PouType, VariableClass};

pub fn generate_iec_variables_to_string(variables: &[PlcVariable]) -> String {
    if variables.is_empty() {
        return "  VAR\n  END_VAR".to_owned();
    }

    let mut groups: BTreeMap<&'static str, Vec<&PlcVariable>> = BTreeMap::new();
    for variable in variables {
        groups
            .entry(class_group(variable.class.unwrap_or(VariableClass::Global)))
            .or_default()
            .push(variable);
    }

    let mut output = String::new();
    for group in [
        "global", "external", "input", "output", "inout", "local", "temp",
    ] {
        let Some(variables) = groups.get(group) else {
            continue;
        };
        output.push_str("  ");
        output.push_str(block_header(group));
        output.push('\n');
        for variable in variables {
            output.push_str("    ");
            output.push_str(&variable.name);
            output.push_str(" : ");
            output.push_str(&variable.variable_type.value);
            if !variable.location.is_empty() {
                output.push_str(" AT ");
                output.push_str(&variable.location);
            }
            if let Some(initial) = variable
                .initial_value
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                output.push_str(" := ");
                output.push_str(initial);
            }
            output.push(';');
            let documentation = variable.documentation.replace(['\r', '\n'], " ");
            let documentation = documentation.trim();
            if !documentation.is_empty() {
                output.push_str(" (* ");
                output.push_str(documentation);
                output.push_str(" *)");
            }
            output.push('\n');
        }
        output.push_str("  END_VAR\n");
    }
    output.trim_end().to_owned()
}

pub fn serialize_pou_to_text(pou: &ParsedPou, variables_text: Option<&str>) -> String {
    let mut output = String::new();
    let documentation = pou.documentation.trim();
    if !documentation.is_empty() {
        output.push_str("(* ");
        output.push_str(documentation);
        output.push_str(" *)\n\n");
    }

    output.push_str(pou.pou_type.start_keyword());
    output.push(' ');
    output.push_str(&pou.name);
    if pou.pou_type == PouType::Function {
        if let Some(return_type) = pou.interface.return_type.as_deref() {
            output.push_str(" : ");
            output.push_str(return_type);
        }
    }
    output.push('\n');

    let generated_variables;
    let variables = if let Some(raw) = variables_text {
        raw
    } else {
        generated_variables = generate_iec_variables_to_string(&pou.interface.variables);
        &generated_variables
    };
    output.push_str(variables);

    match pou.body.language {
        PouLanguage::St | PouLanguage::Il => {
            output.push_str("\n\n");
            output.push_str(pou.body.value.as_str().unwrap_or_default());
            output.push_str("\n\n");
        }
        PouLanguage::Python | PouLanguage::Cpp => {
            output.push('\n');
            output.push_str(pou.body.value.as_str().unwrap_or_default());
            output.push_str("\n\n");
        }
        PouLanguage::Ld | PouLanguage::Fbd => {
            output.push_str("\n\n");
            let graphical = serde_json::to_string_pretty(&pou.body.value)
                .unwrap_or_else(|_| pou.body.value.to_string());
            output.push_str(&graphical);
            output.push('\n');
        }
    }
    output.push_str(pou.pou_type.end_keyword());
    output
}

fn class_group(class: VariableClass) -> &'static str {
    match class {
        VariableClass::Global => "global",
        VariableClass::External => "external",
        VariableClass::Input => "input",
        VariableClass::Output => "output",
        VariableClass::InOut => "inout",
        VariableClass::Local => "local",
        VariableClass::Temp => "temp",
    }
}

fn block_header(group: &str) -> &'static str {
    match group {
        "global" => "VAR_GLOBAL",
        "external" => "VAR_EXTERNAL",
        "input" => "VAR_INPUT",
        "output" => "VAR_OUTPUT",
        "inout" => "VAR_IN_OUT",
        "local" => "VAR",
        "temp" => "VAR_TEMP",
        _ => "VAR",
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{generate_iec_variables_to_string, serialize_pou_to_text};
    use crate::plc_model::{
        ParsedPou, PlcVariable, PlcVariableType, PouBody, PouInterface, PouLanguage, PouType,
        VariableClass,
    };

    fn simple_pou() -> ParsedPou {
        ParsedPou {
            name: "Main".to_owned(),
            pou_type: PouType::Program,
            interface: PouInterface {
                return_type: None,
                variables: Vec::new(),
            },
            body: PouBody::text(PouLanguage::St, "x := 1;"),
            documentation: String::new(),
        }
    }

    #[test]
    fn matches_openplc_empty_variable_block_and_textual_shape() {
        assert_eq!(generate_iec_variables_to_string(&[]), "  VAR\n  END_VAR");
        assert_eq!(
            serialize_pou_to_text(&simple_pou(), None),
            "PROGRAM Main\n  VAR\n  END_VAR\n\nx := 1;\n\nEND_PROGRAM"
        );
    }

    #[test]
    fn matches_openplc_documentation_function_and_variables() {
        let mut pou = simple_pou();
        pou.name = "MyFunc".to_owned();
        pou.pou_type = PouType::Function;
        pou.interface.return_type = Some("INT".to_owned());
        pou.documentation = "This is a doc".to_owned();
        pou.interface.variables.push(PlcVariable {
            name: "x".to_owned(),
            class: Some(VariableClass::Input),
            variable_type: PlcVariableType::base("INT"),
            location: String::new(),
            initial_value: Some("1".to_owned()),
            documentation: "input value".to_owned(),
            debug: Some(false),
        });
        let serialized = serialize_pou_to_text(&pou, None);
        assert!(serialized.starts_with("(* This is a doc *)\n\nFUNCTION MyFunc : INT\n"));
        assert!(serialized.contains("  VAR_INPUT\n    x : INT := 1; (* input value *)\n  END_VAR"));
        assert!(serialized.ends_with("END_FUNCTION"));
    }

    #[test]
    fn variables_text_overrides_structured_variables() {
        let mut pou = simple_pou();
        pou.interface.variables.push(PlcVariable {
            name: "ignored".to_owned(),
            class: Some(VariableClass::Local),
            variable_type: PlcVariableType::base("BOOL"),
            location: String::new(),
            initial_value: None,
            documentation: String::new(),
            debug: None,
        });
        let serialized = serialize_pou_to_text(&pou, Some("VAR\n  raw : INT;\nEND_VAR"));
        assert!(serialized.contains("VAR\n  raw : INT;\nEND_VAR"));
        assert!(!serialized.contains("ignored"));
    }

    #[test]
    fn graphical_body_matches_openplc_pretty_json_shape() {
        let mut pou = simple_pou();
        pou.body = PouBody {
            language: PouLanguage::Ld,
            value: json!({"name":"Main","rungs":[]}),
        };
        assert_eq!(
            serialize_pou_to_text(&pou, None),
            "PROGRAM Main\n  VAR\n  END_VAR\n\n{\n  \"name\": \"Main\",\n  \"rungs\": []\n}\nEND_PROGRAM"
        );
    }
}
