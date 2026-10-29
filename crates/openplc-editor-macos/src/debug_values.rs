use std::collections::{HashMap, HashSet};

use open_rust_plc_compiler::backend::{DebugLeaf, DebugMap};
use time::OffsetDateTime;

use crate::plc_model::{PlcVariable, PouType, VariableClass, VariableTypeDefinition};
use crate::project::OpenProject;

const DEBUG_STRING_CAP: usize = 126;
const MAX_ARRAY_WATCH_EXPANSION: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DebugVariable {
    pub path: String,
    pub type_name: String,
    pub size: usize,
    pub array: u8,
    pub element: u16,
    pub forced: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DebugValue {
    pub variable: DebugVariable,
    pub display: String,
}

pub(crate) fn watched_debug_variables(
    project: &OpenProject,
    map: &DebugMap,
    forced: &HashSet<String>,
) -> Result<Vec<DebugVariable>, String> {
    let prefixes = watched_debug_prefixes(project)?;
    let mut variables = map
        .leaves
        .iter()
        .filter(|leaf| {
            prefixes
                .iter()
                .any(|prefix| leaf_matches_prefix(&leaf.path, prefix))
        })
        .map(|leaf| debug_variable_from_leaf(leaf, forced))
        .collect::<Result<Vec<_>, _>>()?;
    variables.sort_by(|left, right| left.path.cmp(&right.path));
    variables.dedup_by(|left, right| left.path == right.path);
    Ok(variables)
}

pub(crate) fn decode_debug_value(type_name: &str, bytes: &[u8]) -> Result<String, String> {
    let upper = type_name.trim().to_ascii_uppercase();
    match upper.as_str() {
        "BOOL" => Ok(if byte(bytes, 0)? != 0 {
            "TRUE"
        } else {
            "FALSE"
        }
        .to_owned()),
        "SINT" => Ok((byte(bytes, 0)? as i8).to_string()),
        "USINT" | "BYTE" => Ok(byte(bytes, 0)?.to_string()),
        "INT" => Ok(i16::from_le_bytes(read_array(bytes)?).to_string()),
        "UINT" | "WORD" => Ok(u16::from_le_bytes(read_array(bytes)?).to_string()),
        "DINT" => Ok(i32::from_le_bytes(read_array(bytes)?).to_string()),
        "UDINT" | "DWORD" => Ok(u32::from_le_bytes(read_array(bytes)?).to_string()),
        "LINT" => Ok(i64::from_le_bytes(read_array(bytes)?).to_string()),
        "ULINT" | "LWORD" => Ok(u64::from_le_bytes(read_array(bytes)?).to_string()),
        "REAL" => Ok(format!("{:.6}", f32::from_le_bytes(read_array(bytes)?))),
        "LREAL" => Ok(format!("{:.12}", f64::from_le_bytes(read_array(bytes)?))),
        "TIME" => Ok(format_time_ns(i64::from_le_bytes(read_array(bytes)?))),
        "DATE" => format_date_ns(i64::from_le_bytes(read_array(bytes)?)),
        "TOD" => Ok(format_tod_ns(i64::from_le_bytes(read_array(bytes)?))),
        "DT" => format_dt_ns(i64::from_le_bytes(read_array(bytes)?)),
        "STRING" => decode_string(bytes),
        "WSTRING" => decode_wstring(bytes),
        other => Err(format!("unsupported debug type {other}")),
    }
}

pub(crate) fn encode_force_value(type_name: &str, input: &str) -> Result<Vec<u8>, String> {
    let upper = type_name.trim().to_ascii_uppercase();
    let input = input.trim();
    match upper.as_str() {
        "BOOL" => match input.to_ascii_lowercase().as_str() {
            "true" | "1" => Ok(vec![1]),
            "false" | "0" => Ok(vec![0]),
            _ => Err(format!("Invalid BOOL value: {input:?}")),
        },
        "SINT" => Ok(vec![parse_integer(input)? as i8 as u8]),
        "USINT" | "BYTE" => Ok(vec![parse_integer(input)? as u8]),
        "INT" => Ok((parse_integer(input)? as i16).to_le_bytes().to_vec()),
        "UINT" | "WORD" => Ok((parse_integer(input)? as u16).to_le_bytes().to_vec()),
        "DINT" => Ok((parse_integer(input)? as i32).to_le_bytes().to_vec()),
        "UDINT" | "DWORD" => Ok((parse_integer(input)? as u32).to_le_bytes().to_vec()),
        "LINT" => Ok((parse_integer(input)? as i64).to_le_bytes().to_vec()),
        "ULINT" | "LWORD" => Ok((parse_integer(input)? as u64).to_le_bytes().to_vec()),
        "REAL" => input
            .parse::<f32>()
            .map(|value| value.to_le_bytes().to_vec())
            .map_err(|_| format!("Invalid REAL value: {input:?}")),
        "LREAL" => input
            .parse::<f64>()
            .map(|value| value.to_le_bytes().to_vec())
            .map_err(|_| format!("Invalid LREAL value: {input:?}")),
        "TIME" => Ok(parse_time_ns(input)?.to_le_bytes().to_vec()),
        "STRING" => encode_string(input),
        "DATE" | "TOD" | "DT" | "WSTRING" => Err(format!(
            "forcing {upper} is not supported by the pinned editor codec"
        )),
        other => Err(format!("Unknown base type: {other:?}")),
    }
}

fn watched_debug_prefixes(project: &OpenProject) -> Result<HashSet<String>, String> {
    let mut prefixes = HashSet::new();
    let resource = project
        .project
        .data
        .get("configuration")
        .and_then(|value| value.get("resource"));

    if let Some(globals) = resource
        .and_then(|value| value.get("globalVariables"))
        .and_then(|value| value.as_array())
    {
        for global in globals {
            if global.get("debug").and_then(|value| value.as_bool()) != Some(true) {
                continue;
            }
            if let Some(name) = global.get("name").and_then(|value| value.as_str()) {
                prefixes.insert(name.to_ascii_uppercase());
            }
        }
    }

    let pous = project
        .pous
        .iter()
        .filter_map(|file| file.parsed.as_ref())
        .map(|pou| (pou.name.to_ascii_uppercase(), pou))
        .collect::<HashMap<_, _>>();

    let Some(instances) = resource
        .and_then(|value| value.get("instances"))
        .and_then(|value| value.as_array())
    else {
        return Ok(prefixes);
    };

    for instance in instances {
        let Some(instance_name) = instance.get("name").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(program_name) = instance.get("program").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(program) = pous.get(&program_name.to_ascii_uppercase()).copied() else {
            continue;
        };
        if program.pou_type != PouType::Program {
            continue;
        }
        collect_pou_watch_prefixes(
            program,
            &instance_name.to_ascii_uppercase(),
            &pous,
            &mut HashSet::new(),
            &mut prefixes,
        );
    }

    Ok(prefixes)
}

fn collect_pou_watch_prefixes<'a>(
    pou: &'a crate::plc_model::ParsedPou,
    root: &str,
    pous: &HashMap<String, &'a crate::plc_model::ParsedPou>,
    visiting: &mut HashSet<String>,
    output: &mut HashSet<String>,
) {
    let pou_key = pou.name.to_ascii_uppercase();
    if !visiting.insert(pou_key.clone()) {
        return;
    }

    for variable in &pou.interface.variables {
        let external = variable.class == Some(VariableClass::External);
        let variable_root = if external {
            variable.name.to_ascii_uppercase()
        } else {
            format!("{root}.{}", variable.name.to_ascii_uppercase())
        };
        if variable.debug == Some(true) {
            output.insert(variable_root.clone());
        }
        if external {
            continue;
        }

        let Some(function_block_name) = function_block_type_name(variable, pous) else {
            continue;
        };
        let Some(function_block) = pous.get(&function_block_name).copied() else {
            continue;
        };
        let child_roots = function_block_instance_roots(&variable_root, variable);
        for child_root in child_roots {
            collect_pou_watch_prefixes(function_block, &child_root, pous, visiting, output);
        }
    }

    visiting.remove(&pou_key);
}

fn function_block_type_name(
    variable: &PlcVariable,
    pous: &HashMap<String, &crate::plc_model::ParsedPou>,
) -> Option<String> {
    let candidate = match variable.variable_type.definition {
        VariableTypeDefinition::Array => variable
            .variable_type
            .data
            .as_ref()
            .map(|data| data.base_type.value.as_str())?,
        _ => variable.variable_type.value.as_str(),
    }
    .to_ascii_uppercase();
    pous.get(&candidate)
        .filter(|pou| pou.pou_type == PouType::FunctionBlock)
        .map(|_| candidate)
}

fn function_block_instance_roots(base: &str, variable: &PlcVariable) -> Vec<String> {
    if variable.variable_type.definition != VariableTypeDefinition::Array {
        return vec![base.to_owned()];
    }
    let Some(data) = variable.variable_type.data.as_ref() else {
        return vec![base.to_owned()];
    };
    let mut roots = vec![base.to_owned()];
    for dimension in &data.dimensions {
        let Some((start, end)) = parse_dimension(&dimension.dimension) else {
            return vec![base.to_owned()];
        };
        let count = (end - start + 1).max(0) as usize;
        if count == 0 || roots.len().saturating_mul(count) > MAX_ARRAY_WATCH_EXPANSION {
            return vec![base.to_owned()];
        }
        roots = roots
            .into_iter()
            .flat_map(|prefix| (start..=end).map(move |index| format!("{prefix}[{index}]")))
            .collect();
    }
    roots
}

fn parse_dimension(value: &str) -> Option<(i64, i64)> {
    let (start, end) = value.split_once("..")?;
    Some((start.trim().parse().ok()?, end.trim().parse().ok()?))
}

fn leaf_matches_prefix(path: &str, prefix: &str) -> bool {
    path.eq_ignore_ascii_case(prefix)
        || path.get(prefix.len()..).is_some_and(|tail| {
            path[..prefix.len()].eq_ignore_ascii_case(prefix)
                && (tail.starts_with('.') || tail.starts_with('['))
        })
}

fn debug_variable_from_leaf(
    leaf: &DebugLeaf,
    forced: &HashSet<String>,
) -> Result<DebugVariable, String> {
    Ok(DebugVariable {
        path: leaf.path.clone(),
        type_name: leaf.type_name.clone(),
        size: leaf.size,
        array: u8::try_from(leaf.array_idx)
            .map_err(|_| format!("debug array index {} exceeds runtime ABI", leaf.array_idx))?,
        element: u16::try_from(leaf.elem_idx)
            .map_err(|_| format!("debug element index {} exceeds runtime ABI", leaf.elem_idx))?,
        forced: forced.contains(&leaf.path),
    })
}

fn read_array<const N: usize>(bytes: &[u8]) -> Result<[u8; N], String> {
    bytes
        .get(..N)
        .ok_or_else(|| {
            format!(
                "debug payload is {} bytes; expected at least {N}",
                bytes.len()
            )
        })?
        .try_into()
        .map_err(|_| "debug payload width mismatch".to_owned())
}

fn byte(bytes: &[u8], index: usize) -> Result<u8, String> {
    bytes
        .get(index)
        .copied()
        .ok_or_else(|| format!("debug payload is too short for byte {index}"))
}

fn decode_string(bytes: &[u8]) -> Result<String, String> {
    let length = usize::from(byte(bytes, 0)?).min(DEBUG_STRING_CAP);
    let data = bytes
        .get(1..1 + length)
        .ok_or_else(|| "debug STRING payload is truncated".to_owned())?;
    Ok(format!("\"{}\"", String::from_utf8_lossy(data)))
}

fn decode_wstring(bytes: &[u8]) -> Result<String, String> {
    let length = usize::from(byte(bytes, 0)?).min(DEBUG_STRING_CAP);
    if bytes.len() < 1 + length * 2 {
        return Err("debug WSTRING payload is truncated".to_owned());
    }
    let mut units = Vec::with_capacity(length);
    for index in 0..length {
        let offset = 1 + index * 2;
        units.push(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]));
    }
    Ok(format!("\"{}\"", String::from_utf16_lossy(&units)))
}

fn format_time_ns(total_ns: i64) -> String {
    if total_ns == 0 {
        return "0s".to_owned();
    }
    let negative = total_ns < 0;
    let mut ns = i128::from(total_ns).unsigned_abs();
    let units = [
        (86_400_000_000_000u128, "d"),
        (3_600_000_000_000, "h"),
        (60_000_000_000, "m"),
        (1_000_000_000, "s"),
        (1_000_000, "ms"),
        (1_000, "us"),
        (1, "ns"),
    ];
    let mut parts = Vec::new();
    for (factor, suffix) in units {
        let value = ns / factor;
        ns %= factor;
        if value > 0 {
            parts.push(format!("{value}{suffix}"));
            if parts.len() == 2 {
                break;
            }
        }
    }
    let value = parts.join("");
    if negative {
        format!("-{value}")
    } else {
        value
    }
}

fn format_date_ns(total_ns: i64) -> Result<String, String> {
    let date = OffsetDateTime::from_unix_timestamp_nanos(i128::from(total_ns))
        .map_err(|error| format!("invalid DATE value: {error}"))?
        .date();
    Ok(format!(
        "D#{:04}-{:02}-{:02}",
        date.year(),
        u8::from(date.month()),
        date.day()
    ))
}

fn format_dt_ns(total_ns: i64) -> Result<String, String> {
    let dt = OffsetDateTime::from_unix_timestamp_nanos(i128::from(total_ns))
        .map_err(|error| format!("invalid DT value: {error}"))?;
    Ok(format!(
        "DT#{:04}-{:02}-{:02}-{:02}:{:02}:{:02}.{:03}",
        dt.year(),
        u8::from(dt.month()),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second(),
        dt.millisecond()
    ))
}

fn format_tod_ns(total_ns: i64) -> String {
    const DAY: i128 = 86_400_000_000_000;
    let mut ns = i128::from(total_ns) % DAY;
    if ns < 0 {
        ns += DAY;
    }
    let hour = ns / 3_600_000_000_000;
    ns %= 3_600_000_000_000;
    let minute = ns / 60_000_000_000;
    ns %= 60_000_000_000;
    let second = ns / 1_000_000_000;
    let millis = (ns % 1_000_000_000) / 1_000_000;
    format!("TOD#{hour:02}:{minute:02}:{second:02}.{millis:03}")
}

fn encode_string(input: &str) -> Result<Vec<u8>, String> {
    let text = input
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
        .unwrap_or(input);
    if !text.is_ascii() {
        return Err(format!("STRING value must be ASCII: {input:?}"));
    }
    if text.len() > DEBUG_STRING_CAP {
        return Err(format!(
            "STRING value too long: {} characters (max {DEBUG_STRING_CAP})",
            text.len()
        ));
    }
    let mut bytes = vec![0; 1 + DEBUG_STRING_CAP];
    bytes[0] = text.len() as u8;
    bytes[1..1 + text.len()].copy_from_slice(text.as_bytes());
    Ok(bytes)
}

fn parse_integer(input: &str) -> Result<i128, String> {
    let trimmed = input.trim();
    let (negative, body) = trimmed
        .strip_prefix('-')
        .map(|body| (true, body))
        .unwrap_or((false, trimmed));
    let value = if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        i128::from_str_radix(hex, 16)
    } else if let Some(hex) = body.strip_prefix("16#") {
        i128::from_str_radix(hex, 16)
    } else {
        body.parse::<i128>()
    }
    .map_err(|_| format!("Invalid integer value: {input:?}"))?;
    Ok(if negative { -value } else { value })
}

fn parse_time_ns(input: &str) -> Result<i64, String> {
    let trimmed = input.trim();
    let (negative, mut rest) = trimmed
        .strip_prefix('-')
        .map(|body| (true, body))
        .unwrap_or((false, trimmed));
    rest = rest
        .strip_prefix("T#")
        .or_else(|| rest.strip_prefix("t#"))
        .or_else(|| rest.strip_prefix("TIME#"))
        .or_else(|| rest.strip_prefix("time#"))
        .unwrap_or(rest);
    let mut total = 0f64;
    while !rest.is_empty() {
        let number_len = rest
            .char_indices()
            .take_while(|(_, ch)| ch.is_ascii_digit() || *ch == '_' || *ch == '.')
            .map(|(index, ch)| index + ch.len_utf8())
            .last()
            .unwrap_or(0);
        if number_len == 0 {
            return Err(format!("Invalid TIME value: {input:?}"));
        }
        let value = rest[..number_len]
            .replace('_', "")
            .parse::<f64>()
            .map_err(|_| format!("Invalid TIME value: {input:?}"))?;
        rest = &rest[number_len..];
        let lower = rest.to_ascii_lowercase();
        let (factor, unit_len) = if lower.starts_with("ms") {
            (1_000_000f64, 2)
        } else if lower.starts_with("us") {
            (1_000f64, 2)
        } else if lower.starts_with("ns") {
            (1f64, 2)
        } else {
            let unit = rest
                .chars()
                .next()
                .ok_or_else(|| format!("Invalid TIME value: {input:?}"))?;
            let factor = match unit.to_ascii_lowercase() {
                'd' => 86_400_000_000_000f64,
                'h' => 3_600_000_000_000f64,
                'm' => 60_000_000_000f64,
                's' => 1_000_000_000f64,
                _ => return Err(format!("Invalid TIME unit in {input:?}")),
            };
            (factor, unit.len_utf8())
        };
        total += value * factor;
        rest = &rest[unit_len..];
    }
    let total = total.round() as i64;
    Ok(if negative { -total } else { total })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::PathBuf;

    use open_rust_plc_compiler::backend::{DebugArray, DebugLeaf, DebugMap};
    use serde_json::json;

    use super::{decode_debug_value, encode_force_value, watched_debug_variables};
    use crate::plc_model::{
        ParsedPou, PlcVariable, PlcVariableType, PouBody, PouInterface, PouLanguage, PouType,
        VariableClass,
    };
    use crate::project::{
        OpenProject, PouFile, PouKind, ProjectDocument, ProjectKind, ProjectMeta,
    };

    fn variable(name: &str, class: VariableClass, type_name: &str, debug: bool) -> PlcVariable {
        PlcVariable {
            name: name.to_owned(),
            class: Some(class),
            variable_type: PlcVariableType::base(type_name),
            location: String::new(),
            initial_value: None,
            documentation: String::new(),
            debug: Some(debug),
        }
    }

    fn pou(name: &str, pou_type: PouType, variables: Vec<PlcVariable>) -> ParsedPou {
        ParsedPou {
            name: name.to_owned(),
            pou_type,
            interface: PouInterface {
                return_type: None,
                variables,
            },
            body: PouBody::text(PouLanguage::St, ""),
            documentation: String::new(),
        }
    }

    #[test]
    fn filters_manifest_to_project_debug_flags_and_nested_fb_instances() {
        let motor = pou(
            "Motor",
            PouType::FunctionBlock,
            vec![variable("speed", VariableClass::Output, "INT", true)],
        );
        let mut motor_instance = variable("motor0", VariableClass::Local, "Motor", false);
        motor_instance.variable_type = PlcVariableType::derived("Motor");
        let main = pou(
            "Main",
            PouType::Program,
            vec![
                variable("counter", VariableClass::Local, "INT", true),
                motor_instance,
                variable("shared", VariableClass::External, "BOOL", true),
            ],
        );
        let root = PathBuf::from("/tmp/debug-project");
        let project = OpenProject {
            root: root.clone(),
            project: ProjectDocument {
                meta: ProjectMeta {
                    name: "DebugProject".to_owned(),
                    kind: ProjectKind::PlcProject,
                    extra: serde_json::Map::new(),
                },
                data: json!({
                    "configuration": {"resource": {
                        "tasks": [],
                        "instances": [{"name":"instance0","program":"Main","task":"task0"}],
                        "globalVariables": [{"name":"globalWatch","debug":true}]
                    }}
                }),
                extra: serde_json::Map::new(),
            },
            pous: vec![
                PouFile {
                    name: "Main".to_owned(),
                    kind: PouKind::Program,
                    language: Some(PouLanguage::St),
                    relative_path: PathBuf::from("pous/programs/Main.st"),
                    absolute_path: root.join("pous/programs/Main.st"),
                    parsed: Some(main),
                    variables_text: None,
                    raw_content: String::new(),
                },
                PouFile {
                    name: "Motor".to_owned(),
                    kind: PouKind::FunctionBlock,
                    language: Some(PouLanguage::St),
                    relative_path: PathBuf::from("pous/function-blocks/Motor.st"),
                    absolute_path: root.join("pous/function-blocks/Motor.st"),
                    parsed: Some(motor),
                    variables_text: None,
                    raw_content: String::new(),
                },
            ],
            warnings: Vec::new(),
            device_configuration: json!({}),
            device_pin_mapping: json!({}),
            library_manifest: None,
        };
        let leaves = [
            ("GLOBALWATCH", "BOOL"),
            ("INSTANCE0.COUNTER", "INT"),
            ("INSTANCE0.MOTOR0.SPEED", "INT"),
            ("SHARED", "BOOL"),
            ("INSTANCE0.UNWATCHED", "INT"),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (path, type_name))| DebugLeaf {
            array_idx: 0,
            elem_idx: index,
            path: path.to_owned(),
            type_name: type_name.to_owned(),
            size: if type_name == "BOOL" { 1 } else { 2 },
        })
        .collect::<Vec<_>>();
        let map = DebugMap {
            version: 2,
            md5: String::new(),
            type_tags: Default::default(),
            arrays: vec![DebugArray {
                index: 0,
                count: leaves.len(),
            }],
            leaves,
        };
        let variables = watched_debug_variables(&project, &map, &HashSet::new()).unwrap();
        let paths = variables
            .into_iter()
            .map(|variable| variable.path)
            .collect::<HashSet<_>>();
        assert_eq!(
            paths,
            HashSet::from([
                "GLOBALWATCH".to_owned(),
                "INSTANCE0.COUNTER".to_owned(),
                "INSTANCE0.MOTOR0.SPEED".to_owned(),
                "SHARED".to_owned(),
            ])
        );
    }

    #[test]
    fn decodes_pinned_wire_shapes() {
        assert_eq!(decode_debug_value("BOOL", &[1]).unwrap(), "TRUE");
        assert_eq!(
            decode_debug_value("INT", &256i16.to_le_bytes()).unwrap(),
            "256"
        );
        assert_eq!(
            decode_debug_value("TIME", &10_000_000_000i64.to_le_bytes()).unwrap(),
            "10s"
        );
        let mut string = vec![0; 127];
        string[0] = 2;
        string[1..3].copy_from_slice(b"hi");
        assert_eq!(decode_debug_value("STRING", &string).unwrap(), "\"hi\"");
    }

    #[test]
    fn encodes_force_values_in_local_target_byte_order() {
        assert_eq!(encode_force_value("BOOL", "TRUE").unwrap(), vec![1]);
        assert_eq!(encode_force_value("INT", "256").unwrap(), vec![0, 1]);
        assert_eq!(
            encode_force_value("TIME", "T#250ms").unwrap(),
            250_000_000i64.to_le_bytes().to_vec()
        );
        let string = encode_force_value("STRING", "'hi'").unwrap();
        assert_eq!(string.len(), 127);
        assert_eq!(&string[..3], &[2, b'h', b'i']);
        assert!(encode_force_value("DT", "DT#2026-01-01-00:00:00").is_err());
    }
}
