//! Bounded plugin configuration parsing from OpenPLC `plugin_config.c`.

use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

const MAX_PLUGIN_NAME_LEN: usize = 64;
const MAX_PLUGIN_PATH_LEN: usize = 256;

/// One parsed OpenPLC plugin configuration record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginConfig {
    pub name: String,
    pub path: String,
    pub enabled: i32,
    pub plugin_type: i32,
    pub config_path: String,
    pub venv_path: String,
}

fn bounded(value: &str, capacity: usize) -> String {
    let end = value.len().min(capacity - 1);
    String::from_utf8_lossy(&value.as_bytes()[..end]).into_owned()
}

fn trim_c_line_end(value: &str) -> &str {
    value.trim_end_matches(['\n', '\r', ' ', '\t'])
}

fn atoi_compat(value: &str) -> i32 {
    let value = value.trim_start_matches([' ', '\t']);
    let (sign, digits) = match value.strip_prefix('-') {
        Some(rest) => (-1_i64, rest),
        None => match value.strip_prefix('+') {
            Some(rest) => (1_i64, rest),
            None => (1_i64, value),
        },
    };
    let digits: String = digits.chars().take_while(char::is_ascii_digit).collect();
    digits
        .parse::<i64>()
        .ok()
        .map(|number| {
            number
                .saturating_mul(sign)
                .clamp(i32::MIN as i64, i32::MAX as i64) as i32
        })
        .unwrap_or(0)
}

/// Parse plugin CSV text using the field order and bounds of OpenPLC v4.
/// Malformed records are skipped like the C parser; no plugin is loaded.
pub fn parse_plugin_config(text: &str, max_configs: usize) -> Vec<PluginConfig> {
    let mut configs = Vec::new();
    for line in text.lines() {
        if configs.len() == max_configs {
            break;
        }
        if line.starts_with('#') || line.starts_with('\r') || line.is_empty() {
            continue;
        }
        let mut fields = line.split(',');
        let (Some(name), Some(path), Some(enabled), Some(plugin_type)) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let config_path = fields.next().map(trim_c_line_end).unwrap_or("");
        let venv_path = fields.next().map(trim_c_line_end).unwrap_or("");
        configs.push(PluginConfig {
            name: bounded(trim_c_line_end(name), MAX_PLUGIN_NAME_LEN),
            path: bounded(trim_c_line_end(path), MAX_PLUGIN_PATH_LEN),
            enabled: atoi_compat(enabled),
            plugin_type: atoi_compat(plugin_type),
            config_path: bounded(config_path, MAX_PLUGIN_PATH_LEN),
            venv_path: bounded(venv_path, MAX_PLUGIN_PATH_LEN),
        });
    }
    configs
}

/// Read one local plugin-config file and parse it without loading plugins.
pub fn parse_plugin_config_file(
    path: impl AsRef<Path>,
    max_configs: usize,
) -> io::Result<Vec<PluginConfig>> {
    let reader = BufReader::new(File::open(path)?);
    let mut configs = Vec::new();
    for line in reader.lines() {
        if configs.len() == max_configs {
            break;
        }
        configs.extend(parse_plugin_config(&line?, max_configs - configs.len()));
    }
    Ok(configs)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{parse_plugin_config, parse_plugin_config_file, PluginConfig};

    #[test]
    fn parses_bounded_records_and_optional_fields() {
        let configs = parse_plugin_config(
            "# disabled\nfake,./fake.so,1,1,/tmp/config.json,/tmp/venv\nshort,./x,0,0\n",
            10,
        );
        assert_eq!(
            configs,
            vec![
                PluginConfig {
                    name: "fake".into(),
                    path: "./fake.so".into(),
                    enabled: 1,
                    plugin_type: 1,
                    config_path: "/tmp/config.json".into(),
                    venv_path: "/tmp/venv".into(),
                },
                PluginConfig {
                    name: "short".into(),
                    path: "./x".into(),
                    enabled: 0,
                    plugin_type: 0,
                    config_path: String::new(),
                    venv_path: String::new(),
                },
            ]
        );
    }

    #[test]
    fn skips_malformed_lines_and_honors_maximum() {
        let text = "bad\na,one,1,0\nb,two,1x,0,\nc,three,1,0\n";
        let configs = parse_plugin_config(text, 2);
        assert_eq!(configs.len(), 2);
        assert_eq!(configs[1].enabled, 1);
    }

    #[test]
    fn reports_file_open_errors() {
        assert!(parse_plugin_config_file(Path::new("missing.conf"), 1).is_err());
    }
}
