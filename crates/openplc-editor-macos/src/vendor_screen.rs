use std::collections::{BTreeMap, HashSet};

use serde_json::{json, Map, Value};

use crate::project::OpenProject;

pub fn persistence_keys(screen: &Value) -> Vec<String> {
    let Some(sections) = screen.get("sections").and_then(Value::as_array) else {
        return Vec::new();
    };
    sections
        .iter()
        .filter_map(|section| {
            section
                .get("persistence")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    section
                        .get("id")
                        .and_then(Value::as_str)
                        .filter(|value| !value.is_empty())
                })
                .map(str::to_owned)
        })
        .collect()
}

pub fn owned_slice(vendor_data: &Value, keys: &[String]) -> Value {
    let source = vendor_data.as_object();
    let mut result = Map::new();
    let mut ordered = keys.to_vec();
    ordered.sort();
    ordered.dedup();
    for key in ordered {
        if let Some(value) = source.and_then(|source| source.get(&key)) {
            result.insert(key, value.clone());
        }
    }
    Value::Object(result)
}

pub fn replace_owned_slice(vendor_data: &mut Value, keys: &[String], replacement: &Value) {
    if !vendor_data.is_object() {
        *vendor_data = json!({});
    }
    let Some(target) = vendor_data.as_object_mut() else {
        return;
    };
    for key in keys {
        target.remove(key);
    }
    if let Some(replacement) = replacement.as_object() {
        for key in keys {
            if let Some(value) = replacement.get(key) {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}

pub fn set_project_vendor_slice(project: &mut OpenProject, keys: &[String], replacement: &Value) {
    let Some(configuration) = project.device_configuration.as_object_mut() else {
        return;
    };
    let vendor_data = configuration
        .entry("vendorScreenData".to_owned())
        .or_insert_with(|| json!({}));
    replace_owned_slice(vendor_data, keys, replacement);
}

pub fn evaluate_visible(condition: Option<&Value>, values: &Map<String, Value>) -> bool {
    let Some(condition) = condition.and_then(Value::as_object) else {
        return true;
    };
    if let Some(conditions) = condition.get("conditions").and_then(Value::as_array) {
        let mut results = conditions
            .iter()
            .map(|item| evaluate_visible(Some(item), values));
        return if condition.get("operator").and_then(Value::as_str) == Some("and") {
            results.all(|value| value)
        } else {
            results.any(|value| value)
        };
    }
    let key = condition
        .get("condition")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .strip_prefix("fields.")
        .unwrap_or_else(|| {
            condition
                .get("condition")
                .and_then(Value::as_str)
                .unwrap_or_default()
        });
    let current = values.get(key).unwrap_or(&Value::Null);
    let expected = condition.get("value").unwrap_or(&Value::Null);
    match condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "equals" => current == expected,
        "not-equals" => current != expected,
        "in" => expected
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item == current)),
        "exists" => match current {
            Value::Null => false,
            Value::String(value) => !value.is_empty(),
            _ => true,
        },
        "not-exists" => match current {
            Value::Null => true,
            Value::String(value) => value.is_empty(),
            _ => false,
        },
        "greater-than" => current
            .as_f64()
            .zip(expected.as_f64())
            .is_some_and(|(left, right)| left > right),
        "less-than" => current
            .as_f64()
            .zip(expected.as_f64())
            .is_some_and(|(left, right)| left < right),
        _ => true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoChannel {
    pub slot: usize,
    pub module_id: String,
    pub module_name: String,
    pub channel_name: String,
    pub channel_type: String,
    pub data_type: String,
    pub address_prefix: String,
    pub iec_address: String,
    pub alias: String,
}

pub fn regenerate_io_mapping(
    project: &OpenProject,
    module_system: Option<&Value>,
    vendor_data: &Value,
) -> Vec<IoChannel> {
    let modules = module_system
        .and_then(|value| value.get("modules"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let module_by_id = modules
        .iter()
        .filter_map(|module| Some((module.get("id")?.as_str()?.to_owned(), module.clone())))
        .collect::<BTreeMap<_, _>>();
    let module_config = vendor_data
        .get("module-configuration")
        .and_then(Value::as_object);
    let slots = module_config
        .and_then(|value| value.get("slots"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let slots_config = module_config
        .and_then(|value| value.get("slotsConfig"))
        .and_then(Value::as_object);
    let previous_aliases = vendor_data
        .get("io-mapping")
        .and_then(|value| value.get("entries"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some((
                format!(
                    "{}:{}",
                    entry.get("slot")?.as_u64()?,
                    entry.get("channelName")?.as_str()?
                ),
                entry
                    .get("alias")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
            ))
        })
        .collect::<BTreeMap<_, _>>();

    let mut used = collect_used_addresses(project, vendor_data);
    let mut output = Vec::new();
    for (index, module_id) in slots.iter().enumerate() {
        let Some(module_id) = module_id.as_str() else {
            continue;
        };
        let Some(module) = module_by_id.get(module_id) else {
            continue;
        };
        let config = slots_config
            .and_then(|items| items.get(&(index + 1).to_string()))
            .and_then(Value::as_object);
        for channel in resolve_channels(module, config) {
            let prefix = channel
                .get("addressPrefix")
                .and_then(Value::as_str)
                .unwrap_or("%IX");
            let address = next_free_address(prefix, &mut used);
            let channel_name = channel
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Channel")
                .to_owned();
            let alias_key = format!("{}:{channel_name}", index + 1);
            output.push(IoChannel {
                slot: index + 1,
                module_id: module_id.to_owned(),
                module_name: module
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or(module_id)
                    .to_owned(),
                channel_name,
                channel_type: channel
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                data_type: channel
                    .get("dataType")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                address_prefix: prefix.to_owned(),
                iec_address: address,
                alias: previous_aliases
                    .get(&alias_key)
                    .cloned()
                    .unwrap_or_default(),
            });
        }
    }
    output
}

pub fn io_mapping_json(channels: &[IoChannel]) -> Value {
    Value::Array(
        channels
            .iter()
            .map(|channel| {
                json!({
                    "slot": channel.slot,
                    "moduleId": channel.module_id,
                    "moduleName": channel.module_name,
                    "channelName": channel.channel_name,
                    "channelType": channel.channel_type,
                    "dataType": channel.data_type,
                    "iecAddress": channel.iec_address,
                    "alias": channel.alias,
                })
            })
            .collect(),
    )
}

pub fn alias_is_unique(
    project: &OpenProject,
    vendor_data: &Value,
    alias: &str,
    except_key: &str,
) -> bool {
    if alias.trim().is_empty() {
        return true;
    }
    let needle = alias.trim().to_ascii_lowercase();
    let pin_conflict = project
        .device_pin_mapping
        .as_object()
        .into_iter()
        .flat_map(|mapping| mapping.values())
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(|pin| pin.get("alias").and_then(Value::as_str))
        .any(|candidate| candidate.to_ascii_lowercase() == needle);
    if pin_conflict {
        return false;
    }
    !vendor_data
        .get("io-mapping")
        .and_then(|value| value.get("entries"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| {
            let key = format!(
                "{}:{}",
                entry.get("slot").and_then(Value::as_u64).unwrap_or(0),
                entry
                    .get("channelName")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            );
            key != except_key
        })
        .filter_map(|entry| entry.get("alias").and_then(Value::as_str))
        .all(|candidate| candidate.to_ascii_lowercase() != needle)
}

fn collect_used_addresses(project: &OpenProject, vendor_data: &Value) -> HashSet<String> {
    let mut used = HashSet::new();
    fn visit(value: &Value, used: &mut HashSet<String>) {
        match value {
            Value::String(value) if value.starts_with('%') => {
                used.insert(value.clone());
            }
            Value::Array(values) => values.iter().for_each(|value| visit(value, used)),
            Value::Object(values) => values.values().for_each(|value| visit(value, used)),
            _ => {}
        }
    }
    visit(&project.device_pin_mapping, &mut used);
    if let Some(remote) = project.project.data.get("remoteDevices") {
        visit(remote, &mut used);
    }
    if let Some(entries) = vendor_data.get("io-mapping") {
        visit(entries, &mut used);
    }
    used
}

fn resolve_channels(module: &Value, slot_config: Option<&Map<String, Value>>) -> Vec<Value> {
    let Some(mapping) = module.get("addressMapping").and_then(Value::as_object) else {
        return Vec::new();
    };
    if let (Some(field), Some(by_format)) = (
        mapping.get("formatFieldId").and_then(Value::as_str),
        mapping.get("channelsByFormat").and_then(Value::as_object),
    ) {
        let selected = slot_config
            .and_then(|config| config.get(field))
            .and_then(value_to_key)
            .or_else(|| mapping.get("formatDefault").and_then(value_to_key));
        if let Some(channels) = selected
            .as_deref()
            .and_then(|key| by_format.get(key))
            .and_then(Value::as_array)
        {
            return channels.clone();
        }
    }
    let mut output = mapping
        .get("channels")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some(choices) = mapping.get("perChannelChoices").and_then(Value::as_array) {
        for choice in choices {
            let Some(choice) = choice.as_object() else {
                continue;
            };
            let Some(field) = choice.get("fieldId").and_then(Value::as_str) else {
                continue;
            };
            let selected = slot_config
                .and_then(|config| config.get(field))
                .and_then(value_to_key)
                .or_else(|| choice.get("default").and_then(value_to_key));
            let Some(channel) = selected
                .as_deref()
                .and_then(|key| choice.get("modes").and_then(Value::as_object)?.get(key))
            else {
                continue;
            };
            if channel.is_object() {
                output.push(channel.clone());
            }
        }
    }
    output
}

fn value_to_key(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn next_free_address(prefix: &str, used: &mut HashSet<String>) -> String {
    let bit = matches!(prefix, "%IX" | "%QX");
    for index in 0..1_000_000u64 {
        let candidate = if bit {
            format!("{prefix}{}.{}", index / 8, index % 8)
        } else {
            format!("{prefix}{index}")
        };
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    format!("{prefix}0")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{evaluate_visible, owned_slice, persistence_keys, replace_owned_slice};

    #[test]
    fn persistence_keys_match_pinned_fallback_and_owned_slice_contract() {
        let screen = json!({"sections":[
            {"id":"general","persistence":"hal-config","layout":"form"},
            {"id":"modules","persistence":"module-configuration","layout":"module-slots"},
            {"id":"mapping","persistence":"io-mapping","layout":"io-table"},
            {"id":"fallback","layout":"form"}
        ]});
        let keys = persistence_keys(&screen);
        assert_eq!(
            keys,
            vec![
                "hal-config",
                "module-configuration",
                "io-mapping",
                "fallback"
            ]
        );
        let data =
            json!({"hal-config":{"enabled":true},"other":{"value":1},"fallback":{"mode":"x"}});
        assert_eq!(
            owned_slice(&data, &keys),
            json!({"fallback":{"mode":"x"},"hal-config":{"enabled":true}})
        );
        let mut target = data.clone();
        replace_owned_slice(&mut target, &keys, &json!({"hal-config":{"enabled":false}}));
        assert_eq!(target["hal-config"]["enabled"], false);
        assert_eq!(target["other"]["value"], 1);
        assert!(target.get("fallback").is_none());
    }

    #[test]
    fn visible_conditions_match_pinned_comparison_semantics() {
        let values = json!({"enabled":true,"mode":"rtu","count":3});
        let values = values.as_object().unwrap();
        assert!(evaluate_visible(
            Some(&json!({"condition":"fields.enabled","operator":"equals","value":true})),
            values
        ));
        assert!(!evaluate_visible(
            Some(&json!({"condition":"mode","operator":"not-equals","value":"rtu"})),
            values
        ));
        assert!(evaluate_visible(
            Some(&json!({"operator":"and","conditions":[
                {"condition":"enabled","operator":"exists"},
                {"condition":"count","operator":"greater-than","value":2}
            ]})),
            values
        ));
    }
}
