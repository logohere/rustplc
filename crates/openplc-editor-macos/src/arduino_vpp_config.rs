use serde_json::Value;

pub fn generate_vpp_config(value: Option<&Value>) -> String {
    let mut lines = vec![
        "// vpp_config.h — auto-generated, do not edit by hand.".to_owned(),
        "//".to_owned(),
        "// Carries the user-authored configuration-screen data for this".to_owned(),
        "// build target as C preprocessor #defines.".to_owned(),
        String::new(),
        "#ifndef VPP_CONFIG_H".to_owned(),
        "#define VPP_CONFIG_H".to_owned(),
        String::new(),
    ];
    if let Some(object) = value.and_then(Value::as_object) {
        let mut keys = object.keys().cloned().collect::<Vec<_>>();
        keys.sort();
        for key in keys {
            walk(
                &object[&key],
                &format!("VPP_{}", sanitize(&key, true)),
                &mut lines,
            );
        }
        if lines.last().is_some_and(|line| !line.is_empty()) {
            lines.push(String::new());
        }
    }
    lines.push("#endif // VPP_CONFIG_H".to_owned());
    lines.push(String::new());
    lines.join("\n")
}

fn sanitize(value: &str, top_level: bool) -> String {
    let mut out = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    if top_level
        && out
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit())
    {
        out.insert(0, '_');
    }
    out
}

fn walk(value: &Value, path: &str, lines: &mut Vec<String>) {
    match value {
        Value::Null => {}
        Value::Bool(value) => lines.push(format!("#define {path} {}", usize::from(*value))),
        Value::Number(value) => lines.push(format!("#define {path} {value}")),
        Value::String(value) => {
            lines.push(format!("#define {path} \"{}\"", escape_c_string(value)))
        }
        Value::Array(values) => {
            lines.push(format!("#define {path}_COUNT {}", values.len()));
            if values.is_empty() {
                return;
            }
            let all_scalar = values.iter().all(|value| {
                matches!(
                    value,
                    Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
                )
            });
            if all_scalar {
                let values = values
                    .iter()
                    .map(format_scalar)
                    .collect::<Vec<_>>()
                    .join(", ");
                lines.push(format!("#define {path} {{ {values} }}"));
                return;
            }
            for (index, value) in values.iter().enumerate() {
                walk(value, &format!("{path}_{index}"), lines);
            }
            let expansion = (0..values.len())
                .map(|index| format!("X({index})"))
                .collect::<Vec<_>>()
                .join(" ");
            lines.push(format!("#define {path}_FOREACH(X) {expansion}"));
        }
        Value::Object(object) => {
            let mut keys = object.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            for key in keys {
                walk(
                    &object[&key],
                    &format!("{path}_{}", sanitize(&key, false)),
                    lines,
                );
            }
        }
    }
}

fn format_scalar(value: &Value) -> String {
    match value {
        Value::Null => "0".to_owned(),
        Value::Bool(value) => usize::from(*value).to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => format!("\"{}\"", escape_c_string(value)),
        _ => "0".to_owned(),
    }
}

fn escape_c_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::generate_vpp_config;

    #[test]
    fn emits_deterministic_scalar_and_array_macros() {
        let value = json!({
            "4g-network":{"enabled":true,"name":"a\"b"},
            "module-configuration":{"slots":["A",null,"B"],"slotsConfig":{"1":{"gain":3}}}
        });
        let header = generate_vpp_config(Some(&value));
        assert!(header.contains("#define VPP__4G_NETWORK_ENABLED 1"));
        assert!(header.contains("#define VPP__4G_NETWORK_NAME \"a\\\"b\""));
        assert!(header.contains("#define VPP_MODULE_CONFIGURATION_SLOTS_COUNT 3"));
        assert!(header.contains("#define VPP_MODULE_CONFIGURATION_SLOTS { \"A\", 0, \"B\" }"));
        assert!(header.contains("#define VPP_MODULE_CONFIGURATION_SLOTSCONFIG_1_GAIN 3"));
    }
}
