use serde_json::Value;

use crate::vpp_package::VppDevice;

pub fn generate_arduino_defines(
    device: &VppDevice,
    pins: &[Value],
    program_st: &str,
    md5: &str,
) -> String {
    let mut out = String::new();
    if let Some(defines) = device.hal.define.as_ref() {
        let values = match defines {
            Value::String(value) => vec![value.clone()],
            Value::Array(values) => values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        };
        if !values.is_empty() {
            out.push_str("// Board defines\n");
            for define in values {
                out.push_str("#define ");
                out.push_str(&define);
                out.push('\n');
            }
        }
    }
    out.push_str("\n\n//Program MD5\n#define PROGRAM_MD5 \"");
    out.push_str(md5);
    out.push_str("\"\n\n");

    out.push_str("//IO Config\n");
    let digital_in = pins_of_type(pins, "digitalInput");
    let analog_in = pins_of_type(pins, "analogInput");
    let digital_out = pins_of_type(pins, "digitalOutput");
    let analog_out = pins_of_type(pins, "analogOutput");
    out.push_str(&format!("#define PINMASK_DIN {}\n", digital_in.join(", ")));
    out.push_str(&format!("#define PINMASK_AIN {}\n", analog_in.join(", ")));
    out.push_str(&format!(
        "#define PINMASK_DOUT {}\n",
        digital_out.join(", ")
    ));
    out.push_str(&format!("#define PINMASK_AOUT {}\n", analog_out.join(", ")));
    out.push_str(&format!(
        "#define NUM_DISCRETE_INPUT {}\n",
        digital_in.len()
    ));
    out.push_str(&format!("#define NUM_ANALOG_INPUT {}\n", analog_in.len()));
    out.push_str(&format!(
        "#define NUM_DISCRETE_OUTPUT {}\n",
        digital_out.len()
    ));
    out.push_str(&format!(
        "#define NUM_ANALOG_OUTPUT {}\n\n\n",
        analog_out.len()
    ));

    out.push_str("//Arduino libraries\n");
    if contains_any(
        program_st,
        &[
            "DS18B20;",
            "DS18B20_2_OUT;",
            "DS18B20_3_OUT;",
            "DS18B20_4_OUT;",
            "DS18B20_5_OUT;",
        ],
    ) {
        out.push_str("#define USE_DS18B20_BLOCK\n");
    }
    if program_st.contains("P1AM_INIT;") {
        out.push_str("#define USE_P1AM_BLOCKS\n");
    }
    if program_st.contains("CLOUD_BEGIN;") {
        out.push_str("#define USE_CLOUD_BLOCKS\n");
    }
    if contains_any(program_st, &["MQTT_CONNECT;", "MQTT_CONNECT_AUTH;"]) {
        out.push_str("#define USE_MQTT_BLOCKS\n");
    }
    if contains_any(
        program_st,
        &[
            "ARDUINOCAN_CONF;",
            "ARDUINOCAN_WRITE;",
            "ARDUINOCAN_WRITE_WORD;",
            "ARDUINOCAN_READ;",
        ],
    ) {
        out.push_str("#define USE_ARDUINOCAN_BLOCK\n");
    }
    if contains_any(
        program_st,
        &["STM32CAN_CONF;", "STM32CAN_WRITE;", "STM32CAN_READ;"],
    ) {
        out.push_str("#define USE_STM32CAN_BLOCK\n");
    }
    if contains_any(
        program_st,
        &[
            "SM_8RELAY;",
            "SM_16RELAY;",
            "SM_8DIN;",
            "SM_16DIN;",
            "SM_4REL4IN;",
            "SM_INDUSTRIAL;",
            "SM_RTD;",
            "SM_BAS;",
            "SM_HOME;",
            "SM_8MOSFET;",
        ],
    ) {
        out.push_str("#define USE_SM_BLOCKS\n");
    }
    out
}

fn pins_of_type(pins: &[Value], pin_type: &str) -> Vec<String> {
    pins.iter()
        .filter(|pin| pin.get("pinType").and_then(Value::as_str) == Some(pin_type))
        .filter_map(|pin| pin.get("pin").and_then(Value::as_str))
        .map(str::to_owned)
        .collect()
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::vpp_package::VppDevice;

    use super::generate_arduino_defines;

    #[test]
    fn emits_pinned_md5_pin_masks_and_library_gates() {
        let device: VppDevice = serde_json::from_value(json!({
            "id":"uno","name":"Arduino Uno","target":{"type":"arduino-cli"},
            "hal":{"type":"arduino","define":["BOARD_UNO", "FOO=1"]}
        }))
        .unwrap();
        let pins = vec![
            json!({"pin":"2","pinType":"digitalInput","address":"%IX0.0"}),
            json!({"pin":"3","pinType":"digitalOutput","address":"%QX0.0"}),
            json!({"pin":"A0","pinType":"analogInput","address":"%IW0"}),
        ];
        let out = generate_arduino_defines(&device, &pins, "P1AM_INIT;\nMQTT_CONNECT;", "deadbeef");
        assert!(out.contains("#define BOARD_UNO"));
        assert!(out.contains("#define FOO=1"));
        assert!(out.contains("#define PROGRAM_MD5 \"deadbeef\""));
        assert!(out.contains("#define PINMASK_DIN 2"));
        assert!(out.contains("#define PINMASK_AIN A0"));
        assert!(out.contains("#define PINMASK_DOUT 3"));
        assert!(out.contains("#define USE_P1AM_BLOCKS"));
        assert!(out.contains("#define USE_MQTT_BLOCKS"));
    }
}
