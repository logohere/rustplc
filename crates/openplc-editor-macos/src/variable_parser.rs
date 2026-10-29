use std::collections::HashSet;
use std::sync::OnceLock;

use regex::Regex;

use crate::plc_model::{
    ArrayDimension, ArrayTypeData, PlcVariable, PlcVariableType, VariableClass, VariableTypeBase,
    VariableTypeDefinition,
};

const BASE_TYPE_NAMES: &[&str] = &[
    "BOOL", "BYTE", "WORD", "DWORD", "LWORD", "__XWORD", "SINT", "INT", "DINT", "LINT", "USINT",
    "UINT", "UDINT", "ULINT", "REAL", "LREAL", "TIME", "DATE", "TOD", "DT", "STRING", "WSTRING",
];

fn line_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^\s*(?P<name>[A-Za-z0-9_]+)\s*:\s*(?P<type>[A-Za-z0-9_\s\[\]\.]+?)(?:\s+AT\s+(?P<location>[A-Za-z0-9_\.%]+))?\s*(?::=\s*(?P<initialValue>[^;]+?))?\s*;\s*(?:\(\*\s*(?P<documentation>.*?)\s*\*\))?$",
        )
        .expect("pinned variable declaration regex")
    })
}

fn alternate_line_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^\s*(?P<name>[A-Za-z0-9_]+)\s+AT\s+(?P<location>[A-Za-z0-9_\.%]+)\s*:\s*(?P<type>[A-Za-z0-9_\s\[\]\.]+?)\s*(?::=\s*(?P<initialValue>[^;]+?))?\s*;\s*(?:\(\*\s*(?P<documentation>.*?)\s*\*\))?$",
        )
        .expect("pinned alternate variable declaration regex")
    })
}

fn array_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)^ARRAY\s*\[([^\]]+)\]\s+OF\s+([A-Za-z_][A-Za-z0-9_\.]*)\s*$")
            .expect("pinned array type regex")
    })
}

pub fn parse_iec_string_to_variables(
    iec_string: &str,
    function_blocks: &[String],
) -> Result<Vec<PlcVariable>, String> {
    let mut variables = Vec::new();
    let mut current_class = None;
    let function_blocks = function_blocks
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    for (index, raw_line) in iec_string.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(class) = parse_block_start(line) {
            current_class = Some(class);
            continue;
        }
        if starts_with_keyword(line, "END_VAR") {
            current_class = None;
            continue;
        }
        let Some(class) = current_class else {
            continue;
        };

        let captures = line_regex()
            .captures(line)
            .or_else(|| alternate_line_regex().captures(line))
            .ok_or_else(|| {
                format!(
                    "Syntax error on line {line_number}: \"{line}\". Possible cause: {}.",
                    guess_error_reason(line)
                )
            })?;

        let name = captures
            .name("name")
            .map(|m| m.as_str().trim())
            .unwrap_or_default();
        let location = captures
            .name("location")
            .map(|m| m.as_str().trim())
            .unwrap_or_default();
        let parsed_type = captures
            .name("type")
            .map(|m| m.as_str().trim())
            .unwrap_or_default();
        let initial_value = captures
            .name("initialValue")
            .map(|m| m.as_str().trim().to_owned());
        let documentation = captures
            .name("documentation")
            .map(|m| m.as_str().trim().to_owned())
            .unwrap_or_default();

        if !location.is_empty() && !class.allows_location() {
            return Err(format!(
                "Syntax error on line {line_number}: \"{line}\". Location (\"AT\") is not allowed for variables of class \"{}\". Move \"{name}\" to a VAR block (class LOCAL) or remove the \"AT {location}\" clause.",
                class.openplc_name()
            ));
        }
        if initial_value.is_some() && class == VariableClass::External {
            return Err(format!(
                "Syntax error on line {line_number}: Initial Value (\":=\") is not allowed for variables of class \"EXTERNAL\"."
            ));
        }

        let variable_type = parse_array_type(parsed_type).unwrap_or_else(|| {
            if let Some(base) = canonical_base_type(parsed_type) {
                PlcVariableType::base(base)
            } else if function_blocks.contains(&parsed_type.to_ascii_lowercase()) {
                PlcVariableType::derived(parsed_type)
            } else {
                PlcVariableType::user(parsed_type)
            }
        });

        variables.push(PlcVariable {
            name: name.to_owned(),
            class: Some(class),
            variable_type,
            location: location.to_owned(),
            initial_value,
            documentation,
            debug: Some(false),
        });
    }

    Ok(variables)
}

fn parse_block_start(line: &str) -> Option<VariableClass> {
    [
        ("VAR_INPUT", VariableClass::Input),
        ("VAR_OUTPUT", VariableClass::Output),
        ("VAR_IN_OUT", VariableClass::InOut),
        ("VAR_EXTERNAL", VariableClass::External),
        ("VAR_TEMP", VariableClass::Temp),
        ("VAR_GLOBAL", VariableClass::Global),
        ("VAR", VariableClass::Local),
    ]
    .into_iter()
    .find_map(|(keyword, class)| starts_with_keyword(line, keyword).then_some(class))
}

fn starts_with_keyword(line: &str, keyword: &str) -> bool {
    let Some(prefix) = line.get(..keyword.len()) else {
        return false;
    };
    if !prefix.eq_ignore_ascii_case(keyword) {
        return false;
    }
    line[keyword.len()..]
        .chars()
        .next()
        .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_')
}

fn parse_array_type(type_str: &str) -> Option<PlcVariableType> {
    let captures = array_regex().captures(type_str)?;
    let dimensions = captures
        .get(1)?
        .as_str()
        .split(',')
        .map(str::trim)
        .map(|dimension| ArrayDimension {
            dimension: dimension.to_owned(),
        })
        .collect();
    let base_type = captures.get(2)?.as_str().trim();
    let base_type = if let Some(canonical) = canonical_base_type(base_type) {
        VariableTypeBase {
            definition: VariableTypeDefinition::BaseType,
            value: canonical.to_owned(),
        }
    } else {
        VariableTypeBase {
            definition: VariableTypeDefinition::UserDataType,
            value: base_type.to_owned(),
        }
    };

    Some(PlcVariableType {
        definition: VariableTypeDefinition::Array,
        value: type_str.to_owned(),
        data: Some(ArrayTypeData {
            base_type,
            dimensions,
        }),
    })
}

fn canonical_base_type(value: &str) -> Option<&'static str> {
    let value = value.trim();
    BASE_TYPE_NAMES
        .iter()
        .copied()
        .find(|candidate| candidate.eq_ignore_ascii_case(value))
}

fn guess_error_reason(line: &str) -> &'static str {
    if !line.contains(';') {
        return "missing semicolon (;) at the end of the declaration";
    }
    if !line.contains(':') {
        return "missing colon (:) between name and type";
    }
    if line.chars().any(|ch| {
        !(ch.is_ascii_alphanumeric()
            || ch.is_ascii_whitespace()
            || matches!(
                ch,
                '_' | ':' | ';' | '=' | '%' | '(' | ')' | '/' | '*' | '-' | '.' | '[' | ']'
            ))
    }) {
        return "invalid or unsupported characters";
    }
    "unrecognized declaration format"
}

#[cfg(test)]
mod tests {
    use super::parse_iec_string_to_variables;
    use crate::plc_model::{VariableClass, VariableTypeDefinition};

    #[test]
    fn matches_openplc_basic_and_class_parsing() {
        assert!(parse_iec_string_to_variables("", &[]).unwrap().is_empty());
        let parsed = parse_iec_string_to_variables("VAR\n  counter : int;\nEND_VAR", &[]).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, "counter");
        assert_eq!(parsed[0].class, Some(VariableClass::Local));
        assert_eq!(
            parsed[0].variable_type.definition,
            VariableTypeDefinition::BaseType
        );
        assert_eq!(parsed[0].variable_type.value, "INT");
        assert_eq!(parsed[0].location, "");
        assert_eq!(parsed[0].initial_value, None);
        assert_eq!(parsed[0].documentation, "");
        assert_eq!(parsed[0].debug, Some(false));

        for (block, class) in [
            ("VAR", VariableClass::Local),
            ("VAR_INPUT", VariableClass::Input),
            ("VAR_OUTPUT", VariableClass::Output),
            ("VAR_IN_OUT", VariableClass::InOut),
            ("VAR_EXTERNAL", VariableClass::External),
            ("VAR_TEMP", VariableClass::Temp),
            ("VAR_GLOBAL", VariableClass::Global),
        ] {
            let input = format!("{block}\n  x : INT;\nEND_VAR");
            assert_eq!(
                parse_iec_string_to_variables(&input, &[]).unwrap()[0].class,
                Some(class)
            );
        }
    }

    #[test]
    fn matches_openplc_array_location_value_and_documentation() {
        let parsed = parse_iec_string_to_variables(
            "VAR_GLOBAL\n  arr : ARRAY[0..2] OF BOOL AT %MW0 := TRUE; (* buffer *)\nEND_VAR",
            &[],
        )
        .unwrap();
        let variable = &parsed[0];
        assert_eq!(
            variable.variable_type.definition,
            VariableTypeDefinition::Array
        );
        assert_eq!(variable.variable_type.value, "ARRAY[0..2] OF BOOL");
        let data = variable.variable_type.data.as_ref().unwrap();
        assert_eq!(data.base_type.definition, VariableTypeDefinition::BaseType);
        assert_eq!(data.base_type.value, "BOOL");
        assert_eq!(data.dimensions[0].dimension, "0..2");
        assert_eq!(variable.location, "%MW0");
        assert_eq!(variable.initial_value.as_deref(), Some("TRUE"));
        assert_eq!(variable.documentation, "buffer");
    }

    #[test]
    fn matches_openplc_alternate_location_and_restrictions() {
        let parsed =
            parse_iec_string_to_variables("VAR\n  sensor AT %IX0.0 : BOOL;\nEND_VAR", &[]).unwrap();
        assert_eq!(parsed[0].location, "%IX0.0");

        let error =
            parse_iec_string_to_variables("VAR_OUTPUT\n  actuator AT %QX0.0 : BOOL;\nEND_VAR", &[])
                .unwrap_err();
        assert_eq!(
            error,
            "Syntax error on line 2: \"actuator AT %QX0.0 : BOOL;\". Location (\"AT\") is not allowed for variables of class \"OUTPUT\". Move \"actuator\" to a VAR block (class LOCAL) or remove the \"AT %QX0.0\" clause."
        );
        assert!(
            parse_iec_string_to_variables("VAR_EXTERNAL\n  x : INT := 10;\nEND_VAR", &[])
                .unwrap_err()
                .contains("Initial Value")
        );
    }

    #[test]
    fn matches_openplc_diagnostics_and_derived_detection() {
        for (line, expected) in [
            ("counter : INT", "missing semicolon"),
            ("counter INT;", "missing colon"),
            ("counter : INT @;", "invalid or unsupported characters"),
            (": ;", "unrecognized declaration format"),
        ] {
            let input = format!("VAR\n  {line}\nEND_VAR");
            assert!(parse_iec_string_to_variables(&input, &[])
                .unwrap_err()
                .contains(expected));
        }

        let parsed =
            parse_iec_string_to_variables("VAR\n  inst : myfb;\nEND_VAR", &["MyFB".to_owned()])
                .unwrap();
        assert_eq!(
            parsed[0].variable_type.definition,
            VariableTypeDefinition::Derived
        );
        assert_eq!(parsed[0].variable_type.value, "myfb");
    }
}
