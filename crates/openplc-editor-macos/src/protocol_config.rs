use open_rust_plc_compiler::backend::{DebugLeaf, DebugMap};
use serde_json::{json, Map, Value};

use crate::project::OpenProject;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeConfs {
    pub modbus_slave: Option<String>,
    pub modbus_master: Option<String>,
    pub s7comm: Option<String>,
    pub opcua: Option<String>,
    pub ethercat: Option<String>,
}

pub fn default_server(name: &str, protocol: &str) -> Option<Value> {
    match protocol {
        "modbus-tcp" => Some(json!({
            "name": name,
            "protocol": "modbus-tcp",
            "modbusSlaveConfig": {
                "enabled": false,
                "networkInterface": "0.0.0.0",
                "port": 502
            }
        })),
        "s7comm" => Some(json!({
            "name": name,
            "protocol": "s7comm",
            "s7commSlaveConfig": {
                "server": {
                    "enabled": false,
                    "bindAddress": "0.0.0.0",
                    "port": 102,
                    "maxClients": 32,
                    "workIntervalMs": 100,
                    "sendTimeoutMs": 3000,
                    "recvTimeoutMs": 3000,
                    "pingTimeoutMs": 10000,
                    "pduSize": 480
                },
                "plcIdentity": {
                    "name": "OpenPLC Runtime",
                    "moduleType": "CPU 315-2 PN/DP",
                    "serialNumber": "S C-OPENPLC01",
                    "copyright": "OpenPLC Project",
                    "moduleName": "OpenPLC"
                },
                "dataBlocks": [],
                "logging": {
                    "logConnections": true,
                    "logDataAccess": false,
                    "logErrors": true
                }
            }
        })),
        "opcua" => Some(json!({
            "name": name,
            "protocol": "opcua",
            "opcuaServerConfig": {
                "server": {
                    "enabled": false,
                    "name": "OpenPLC OPC UA Server",
                    "applicationUri": "urn:openplc:opcua:server",
                    "productUri": "urn:openplc:runtime",
                    "bindAddress": "0.0.0.0",
                    "port": 4840,
                    "endpointPath": "/openplc/opcua"
                },
                "securityProfiles": [{
                    "id": "default-insecure",
                    "name": "insecure",
                    "enabled": true,
                    "securityPolicy": "None",
                    "securityMode": "None",
                    "authMethods": ["Anonymous"]
                }],
                "security": {
                    "serverCertificateStrategy": "auto_self_signed",
                    "serverCertificateCustom": null,
                    "serverPrivateKeyCustom": null,
                    "trustedClientCertificates": []
                },
                "users": [],
                "cycleTimeMs": 100,
                "addressSpace": {
                    "namespaceUri": "urn:openplc:opcua:namespace",
                    "nodes": []
                }
            }
        })),
        _ => None,
    }
}

pub fn default_remote_device(name: &str, protocol: &str) -> Option<Value> {
    match protocol {
        "modbus-tcp" => Some(json!({
            "name": name,
            "protocol": "modbus-tcp",
            "modbusTcpConfig": {
                "host": "127.0.0.1",
                "port": 502,
                "slaveId": 1,
                "timeout": 1000,
                "ioGroups": []
            }
        })),
        "ethercat" => Some(json!({
            "name": name,
            "protocol": "ethercat",
            "ethercatConfig": {
                "masterConfig": {
                    "enabled": true,
                    "networkInterface": "eth0",
                    "cycleTimeUs": 1000,
                    "watchdogTimeoutCycles": 3,
                    "taskPriority": 90
                },
                "devices": []
            }
        })),
        "ethernet-ip" | "profinet" => Some(json!({
            "name": name,
            "protocol": protocol
        })),
        _ => None,
    }
}

pub fn generate_runtime_confs(
    project: &OpenProject,
    debug_map: &DebugMap,
) -> Result<RuntimeConfs, String> {
    let servers = project_array(project, "servers");
    let remote_devices = project_array(project, "remoteDevices");
    Ok(RuntimeConfs {
        modbus_slave: generate_modbus_slave(&servers)?,
        modbus_master: generate_modbus_master(&remote_devices)?,
        s7comm: generate_s7comm(&servers)?,
        opcua: generate_opcua(project, &servers, debug_map)?,
        ethercat: generate_ethercat(&remote_devices)?,
    })
}

fn project_array(project: &OpenProject, key: &str) -> Vec<Value> {
    project
        .project
        .data
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn generate_modbus_slave(servers: &[Value]) -> Result<Option<String>, String> {
    let Some(server) = servers.iter().find(|server| {
        server.get("protocol").and_then(Value::as_str) == Some("modbus-tcp")
            && server.get("modbusSlaveConfig").is_some()
    }) else {
        return Ok(None);
    };
    let config = &server["modbusSlaveConfig"];
    let mapping = config.get("bufferMapping");
    let output = json!({
        "network_configuration": {
            "host": string_or(config, "networkInterface", "0.0.0.0"),
            "port": integer_or(config, "port", 502),
        },
        "buffer_mapping": {
            "holding_registers": {
                "qw_count": nested_integer_or(mapping, &["holdingRegisters", "qwCount"], 1024),
                "mw_count": nested_integer_or(mapping, &["holdingRegisters", "mwCount"], 1024),
                "md_count": nested_integer_or(mapping, &["holdingRegisters", "mdCount"], 1024),
                "ml_count": nested_integer_or(mapping, &["holdingRegisters", "mlCount"], 1024),
            },
            "coils": {
                "qx_bits": nested_integer_or(mapping, &["coils", "qxBits"], 8192),
                "mx_bits": nested_integer_or(mapping, &["coils", "mxBits"], 0),
            },
            "discrete_inputs": {
                "ix_bits": nested_integer_or(mapping, &["discreteInputs", "ixBits"], 8192),
            },
            "input_registers": {
                "iw_count": nested_integer_or(mapping, &["inputRegisters", "iwCount"], 1024),
            }
        }
    });
    pretty(output).map(Some)
}

fn generate_modbus_master(devices: &[Value]) -> Result<Option<String>, String> {
    let mut output = Vec::new();
    for device in devices.iter().filter(|device| {
        device.get("protocol").and_then(Value::as_str) == Some("modbus-tcp")
            && device.get("modbusTcpConfig").is_some()
    }) {
        let config = &device["modbusTcpConfig"];
        let groups = config
            .get("ioGroups")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if groups.is_empty() {
            continue;
        }
        let io_points = groups
            .iter()
            .map(|group| {
                let first_location = group
                    .get("ioPoints")
                    .and_then(Value::as_array)
                    .and_then(|points| points.first())
                    .and_then(|point| point.get("iecLocation"))
                    .and_then(Value::as_str)
                    .unwrap_or("%MW0");
                json!({
                    "fc": group.get("functionCode").and_then(Value::as_str).and_then(|value| value.parse::<i64>().ok()).unwrap_or(3),
                    "offset": format_modbus_offset(group.get("offset").and_then(Value::as_str).unwrap_or("0")),
                    "iec_location": first_location,
                    "len": integer_or(group, "length", 1),
                    "cycle_time_ms": integer_or(group, "cycleTime", 100),
                    "error_handling": string_or(group, "errorHandling", "keep-last-value"),
                })
            })
            .collect::<Vec<_>>();
        let transport = string_or(config, "transport", "tcp");
        let common = json!({
            "type": "SLAVE",
            "transport": transport,
            "timeout_ms": integer_or(config, "timeout", 1000),
            "slave_id": integer_or(config, "slaveId", 1),
            "io_points": io_points,
        });
        let mut object = common.as_object().cloned().unwrap_or_default();
        if transport == "rtu" {
            let serial_port = config
                .get("serialPort")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty());
            let Some(serial_port) = serial_port else {
                continue;
            };
            object.insert(
                "serial_port".to_owned(),
                Value::String(serial_port.to_owned()),
            );
            object.insert(
                "baud_rate".to_owned(),
                Value::from(integer_or(config, "baudRate", 9600)),
            );
            object.insert(
                "parity".to_owned(),
                Value::String(string_or(config, "parity", "N")),
            );
            object.insert(
                "stop_bits".to_owned(),
                Value::from(integer_or(config, "stopBits", 1)),
            );
            object.insert(
                "data_bits".to_owned(),
                Value::from(integer_or(config, "dataBits", 8)),
            );
        } else {
            object.insert(
                "host".to_owned(),
                Value::String(string_or(config, "host", "127.0.0.1")),
            );
            object.insert(
                "port".to_owned(),
                Value::from(integer_or(config, "port", 502)),
            );
        }
        output.push(json!({
            "name": string_or(device, "name", "modbus_device"),
            "protocol": "MODBUS",
            "config": Value::Object(object),
        }));
    }
    if output.is_empty() {
        Ok(None)
    } else {
        pretty(Value::Array(output)).map(Some)
    }
}

fn generate_s7comm(servers: &[Value]) -> Result<Option<String>, String> {
    let Some(server) = servers.iter().find(|server| {
        server.get("protocol").and_then(Value::as_str) == Some("s7comm")
            && server.get("s7commSlaveConfig").is_some()
    }) else {
        return Ok(None);
    };
    let config = &server["s7commSlaveConfig"];
    let server_config = &config["server"];
    let data_blocks = config
        .get("dataBlocks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|block| {
            let mapping = &block["mapping"];
            let mut runtime_mapping = json!({
                "type": string_or(mapping, "type", "int_memory"),
                "start_buffer": integer_or(mapping, "startBuffer", 0),
            });
            if mapping.get("bitAddressing").and_then(Value::as_bool) == Some(true) {
                runtime_mapping["bit_addressing"] = Value::Bool(true);
            }
            json!({
                "db_number": integer_or(&block, "dbNumber", 1),
                "description": string_or(&block, "description", ""),
                "size_bytes": integer_or(&block, "sizeBytes", 256),
                "mapping": runtime_mapping,
            })
        })
        .collect::<Vec<_>>();
    let mut output = json!({
        "server": {
            "enabled": server_config.get("enabled").and_then(Value::as_bool).unwrap_or(false),
            "bind_address": string_or(server_config, "bindAddress", "0.0.0.0"),
            "port": integer_or(server_config, "port", 102),
            "max_clients": integer_or(server_config, "maxClients", 32),
            "work_interval_ms": integer_or(server_config, "workIntervalMs", 100),
            "send_timeout_ms": integer_or(server_config, "sendTimeoutMs", 3000),
            "recv_timeout_ms": integer_or(server_config, "recvTimeoutMs", 3000),
            "ping_timeout_ms": integer_or(server_config, "pingTimeoutMs", 10000),
            "pdu_size": integer_or(server_config, "pduSize", 480),
        },
        "data_blocks": data_blocks,
    });
    if let Some(identity) = config.get("plcIdentity") {
        output["plc_identity"] = json!({
            "name": string_or(identity, "name", "OpenPLC Runtime"),
            "module_type": string_or(identity, "moduleType", "CPU 315-2 PN/DP"),
            "serial_number": string_or(identity, "serialNumber", "S C-OPENPLC01"),
            "copyright": string_or(identity, "copyright", "OpenPLC Project"),
            "module_name": string_or(identity, "moduleName", "OpenPLC"),
        });
    }
    if let Some(logging) = config.get("logging") {
        output["logging"] = json!({
            "log_connections": logging.get("logConnections").and_then(Value::as_bool).unwrap_or(true),
            "log_data_access": logging.get("logDataAccess").and_then(Value::as_bool).unwrap_or(false),
            "log_errors": logging.get("logErrors").and_then(Value::as_bool).unwrap_or(true),
        });
    }
    if let Some(areas) = config.get("systemAreas").and_then(Value::as_object) {
        let mut runtime_areas = Map::new();
        for (source, target) in [
            ("peArea", "pe_area"),
            ("paArea", "pa_area"),
            ("mkArea", "mk_area"),
        ] {
            if let Some(area) = areas.get(source) {
                let mut runtime_area = json!({
                    "enabled": area.get("enabled").and_then(Value::as_bool).unwrap_or(false),
                    "size_bytes": integer_or(area, "sizeBytes", 1),
                });
                if let Some(mapping) = area.get("mapping") {
                    runtime_area["mapping"] = json!({
                        "type": string_or(mapping, "type", "int_memory"),
                        "start_buffer": integer_or(mapping, "startBuffer", 0),
                    });
                }
                runtime_areas.insert(target.to_owned(), runtime_area);
            }
        }
        if !runtime_areas.is_empty() {
            output["system_areas"] = Value::Object(runtime_areas);
        }
    }
    pretty(output).map(Some)
}

fn generate_ethercat(devices: &[Value]) -> Result<Option<String>, String> {
    let mut roots = Vec::new();
    for remote in devices.iter().filter(|device| {
        device.get("protocol").and_then(Value::as_str) == Some("ethercat")
            && device.get("ethercatConfig").is_some()
    }) {
        let config = &remote["ethercatConfig"];
        if config
            .get("masterConfig")
            .and_then(|master| master.get("enabled"))
            .and_then(Value::as_bool)
            == Some(false)
        {
            continue;
        }
        let configured = config
            .get("devices")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if configured.is_empty() {
            continue;
        }
        let slaves = configured
            .iter()
            .enumerate()
            .map(|(index, device)| ethercat_slave(device, index))
            .collect::<Result<Vec<_>, _>>()?;
        let master = config.get("masterConfig").unwrap_or(&Value::Null);
        roots.push(json!({
            "name": string_or(remote, "name", "ethercat_master"),
            "protocol": "ETHERCAT",
            "config": {
                "master": {
                    "interface": string_or(master, "networkInterface", "eth0"),
                    "cycle_time_us": integer_or(master, "cycleTimeUs", 1000),
                    "watchdog_timeout_cycles": integer_or(master, "watchdogTimeoutCycles", 3),
                    "task_priority": integer_or(master, "taskPriority", 90),
                },
                "slaves": slaves,
                "diagnostics": {
                    "log_connections": true,
                    "log_data_access": false,
                    "log_errors": true,
                    "max_log_entries": 10000,
                    "status_update_interval_ms": 500
                }
            }
        }));
    }
    if roots.is_empty() {
        Ok(None)
    } else {
        pretty(Value::Array(roots)).map(Some)
    }
}

fn ethercat_slave(device: &Value, index: usize) -> Result<Value, String> {
    let cfg = device.get("config").ok_or_else(|| {
        format!(
            "EtherCAT device {} has no config",
            string_or(device, "name", "unknown")
        )
    })?;
    let channel_info = device
        .get("channelInfo")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mappings = device
        .get("channelMappings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let channels = channel_info
        .iter()
        .enumerate()
        .map(|(channel_index, channel)| {
            let channel_id = string_or(channel, "channelId", "");
            let location = mappings
                .iter()
                .find(|mapping| string_or(mapping, "channelId", "") == channel_id)
                .map(|mapping| string_or(mapping, "iecLocation", ""))
                .unwrap_or_default();
            let bit_length = integer_or(channel, "bitLen", 1);
            let direction = string_or(channel, "direction", "input");
            json!({
                "index": channel_index,
                "name": string_or(channel, "name", ""),
                "type": if direction == "input" {
                    if bit_length == 1 { "digital_input" } else { "analog_input" }
                } else if bit_length == 1 { "digital_output" } else { "analog_output" },
                "bit_length": bit_length,
                "iec_location": location,
                "pdo_index": string_or(channel, "pdoIndex", "0x0000"),
                "pdo_entry_index": string_or(channel, "entryIndex", "0x0000"),
                "pdo_entry_subindex": parse_hex_integer(&string_or(channel, "entrySubIndex", "0")),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "position": device.get("position").and_then(Value::as_i64).unwrap_or(index as i64 + 1),
        "name": string_or(device, "name", ""),
        "type": string_or(device, "slaveType", "coupler"),
        "vendor_id": string_or(device, "vendorId", "0x00000000"),
        "product_code": string_or(device, "productCode", "0x00000000"),
        "revision": string_or(device, "revisionNo", "0x00000000"),
        "config": {
            "startup_checks": {
                "check_vendor_id": bool_nested(cfg, &["startupChecks", "checkVendorId"], true),
                "check_product_code": bool_nested(cfg, &["startupChecks", "checkProductCode"], true),
            },
            "addressing": {
                "ethercat_address": int_nested(cfg, &["addressing", "ethercatAddress"], 0),
            },
            "timeouts": {
                "sdo_timeout_ms": int_nested(cfg, &["timeouts", "sdoTimeoutMs"], 1000),
                "init_to_preop_timeout_ms": int_nested(cfg, &["timeouts", "initToPreOpTimeoutMs"], 5000),
                "safeop_to_op_timeout_ms": int_nested(cfg, &["timeouts", "safeOpToOpTimeoutMs"], 5000),
            },
            "watchdog": {
                "sm_watchdog_enabled": bool_nested(cfg, &["watchdog", "smWatchdogEnabled"], false),
                "sm_watchdog_ms": int_nested(cfg, &["watchdog", "smWatchdogMs"], 100),
                "pdi_watchdog_enabled": bool_nested(cfg, &["watchdog", "pdiWatchdogEnabled"], false),
                "pdi_watchdog_ms": int_nested(cfg, &["watchdog", "pdiWatchdogMs"], 100),
            },
            "distributed_clocks": {
                "enabled": bool_nested(cfg, &["distributedClocks", "dcEnabled"], false),
                "sync_unit_cycle_us": int_nested(cfg, &["distributedClocks", "dcSyncUnitCycleUs"], 1000),
                "sync0_enabled": bool_nested(cfg, &["distributedClocks", "dcSync0Enabled"], false),
                "sync0_cycle_us": int_nested(cfg, &["distributedClocks", "dcSync0CycleUs"], 1000),
                "sync0_shift_us": int_nested(cfg, &["distributedClocks", "dcSync0ShiftUs"], 0),
                "sync1_enabled": bool_nested(cfg, &["distributedClocks", "dcSync1Enabled"], false),
                "sync1_cycle_us": int_nested(cfg, &["distributedClocks", "dcSync1CycleUs"], 1000),
                "sync1_shift_us": int_nested(cfg, &["distributedClocks", "dcSync1ShiftUs"], 0),
            }
        },
        "channels": channels,
        "sdo_configurations": ethercat_sdos(device),
        "rx_pdos": ethercat_pdos(device.get("rxPdos")),
        "tx_pdos": ethercat_pdos(device.get("txPdos")),
    }))
}

fn ethercat_pdos(value: Option<&Value>) -> Value {
    let pdos = value.and_then(Value::as_array).cloned().unwrap_or_default();
    Value::Array(
        pdos.into_iter()
            .map(|pdo| {
                let entries = pdo
                    .get("entries")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|entry| {
                        let index = string_or(&entry, "index", "0x0000");
                        json!({
                            "index": index,
                            "subindex": parse_hex_integer(&string_or(&entry, "subIndex", "0")),
                            "bit_length": integer_or(&entry, "bitLen", 0),
                            "name": string_or(&entry, "name", ""),
                            "data_type": if index == "0x0000" { "PAD".to_owned() } else { string_or(&entry, "dataType", "") },
                        })
                    })
                    .collect::<Vec<_>>();
                json!({
                    "index": string_or(&pdo, "index", "0x0000"),
                    "name": string_or(&pdo, "name", ""),
                    "entries": entries,
                })
            })
            .collect(),
    )
}

fn ethercat_sdos(device: &Value) -> Value {
    let entries = device
        .get("sdoConfigurations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Value::Array(
        entries
            .into_iter()
            .filter(|entry| {
                entry
                    .get("value")
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty())
            })
            .map(|entry| {
                json!({
                    "index": string_or(&entry, "index", "0x0000"),
                    "subindex": integer_or(&entry, "subIndex", 0),
                    "value": parse_numeric_value(&string_or(&entry, "value", "0")),
                    "data_type": string_or(&entry, "dataType", ""),
                    "bit_length": integer_or(&entry, "bitLength", 0),
                    "name": string_or(&entry, "name", ""),
                    "comment": format!("Startup SDO: {}", string_or(&entry, "objectName", "")),
                })
            })
            .collect(),
    )
}

fn generate_opcua(
    project: &OpenProject,
    servers: &[Value],
    debug_map: &DebugMap,
) -> Result<Option<String>, String> {
    let Some(server) = servers.iter().find(|server| {
        server.get("protocol").and_then(Value::as_str) == Some("opcua")
            && server
                .get("opcuaServerConfig")
                .and_then(|config| config.get("server"))
                .and_then(|server| server.get("enabled"))
                .and_then(Value::as_bool)
                == Some(true)
    }) else {
        return Ok(None);
    };
    let config = &server["opcuaServerConfig"];
    let server_config = &config["server"];
    let profiles = config
        .get("securityProfiles")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|profile| profile.get("enabled").and_then(Value::as_bool) == Some(true))
        .map(|profile| {
            json!({
                "name": string_or(&profile, "name", ""),
                "enabled": true,
                "security_policy": string_or(&profile, "securityPolicy", "None"),
                "security_mode": string_or(&profile, "securityMode", "None"),
                "auth_methods": profile.get("authMethods").cloned().unwrap_or_else(|| json!(["Anonymous"])),
            })
        })
        .collect::<Vec<_>>();
    if profiles.is_empty() {
        return Err(
            "OPC-UA configuration requires at least one enabled security profile.".to_owned(),
        );
    }
    let instances = project
        .project
        .data
        .get("configuration")
        .and_then(|configuration| configuration.get("resource"))
        .and_then(|resource| resource.get("instances"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let nodes = config
        .get("addressSpace")
        .and_then(|address_space| address_space.get("nodes"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut variables = Vec::new();
    let mut structures = Vec::new();
    let mut arrays = Vec::new();
    for node in nodes {
        match string_or(&node, "nodeType", "variable").as_str() {
            "array"
                if node
                    .get("fields")
                    .and_then(Value::as_array)
                    .is_some_and(|fields| !fields.is_empty()) =>
            {
                if let Some(structure) = opc_structure(&node, &instances, debug_map)? {
                    structures.push(structure);
                }
            }
            "array" => arrays.push(opc_array(&node, &instances, debug_map)?),
            "structure" => {
                if let Some(structure) = opc_structure(&node, &instances, debug_map)? {
                    structures.push(structure);
                }
            }
            _ => variables.push(opc_variable(&node, &instances, debug_map)?),
        }
    }
    let security = config.get("security").cloned().unwrap_or_else(|| json!({}));
    let users = config
        .get("users")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|user| {
            json!({
                "type": string_or(&user, "type", "password"),
                "username": user.get("username").cloned().unwrap_or(Value::Null),
                "password_hash": user.get("passwordHash").cloned().unwrap_or(Value::Null),
                "certificate_id": user.get("certificateId").cloned().unwrap_or(Value::Null),
                "role": string_or(&user, "role", "viewer"),
            })
        })
        .collect::<Vec<_>>();
    let trusted = security
        .get("trustedClientCertificates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|certificate| {
            json!({
                "id": string_or(&certificate, "id", ""),
                "pem": string_or(&certificate, "pem", ""),
            })
        })
        .collect::<Vec<_>>();
    let endpoint_url = format!(
        "opc.tcp://{}:{}{}",
        string_or(server_config, "bindAddress", "0.0.0.0"),
        integer_or(server_config, "port", 4840),
        string_or(server_config, "endpointPath", "/openplc/opcua")
    );
    let runtime = json!([{
        "name": "opcua_server",
        "protocol": "OPC-UA",
        "config": {
            "format_version": 2,
            "server": {
                "name": string_or(server_config, "name", "OpenPLC OPC UA Server"),
                "application_uri": string_or(server_config, "applicationUri", "urn:openplc:opcua:server"),
                "product_uri": string_or(server_config, "productUri", "urn:openplc:runtime"),
                "endpoint_url": endpoint_url,
                "security_profiles": profiles,
            },
            "security": {
                "server_certificate_strategy": string_or(&security, "serverCertificateStrategy", "auto_self_signed"),
                "server_certificate_custom": security.get("serverCertificateCustom").cloned().unwrap_or(Value::Null),
                "server_private_key_custom": security.get("serverPrivateKeyCustom").cloned().unwrap_or(Value::Null),
                "trusted_client_certificates": trusted,
            },
            "users": users,
            "cycle_time_ms": integer_or(config, "cycleTimeMs", 100),
            "address_space": {
                "namespace_uri": config.get("addressSpace").map(|value| string_or(value, "namespaceUri", "urn:openplc:opcua:namespace")).unwrap_or_else(|| "urn:openplc:opcua:namespace".to_owned()),
                "variables": variables,
                "structures": structures,
                "arrays": arrays,
            }
        }
    }]);
    pretty(runtime).map(Some)
}

fn opc_variable(node: &Value, instances: &[Value], debug_map: &DebugMap) -> Result<Value, String> {
    let leaf = resolve_opc_leaf(node, instances, debug_map)?;
    Ok(json!({
        "node_id": string_or(node, "nodeId", ""),
        "browse_name": string_or(node, "browseName", ""),
        "display_name": string_or(node, "displayName", ""),
        "datatype": leaf.type_name,
        "size": leaf.size,
        "description": string_or(node, "description", ""),
        "arr": leaf.array_idx,
        "elem": leaf.elem_idx,
        "permissions": node.get("permissions").cloned().unwrap_or_else(default_permissions),
    }))
}

fn opc_array(node: &Value, instances: &[Value], debug_map: &DebugMap) -> Result<Value, String> {
    let base = opc_base_path(node, instances)?;
    let prefix = format!("{}[", base.to_ascii_uppercase());
    let leaf = debug_map
        .leaves
        .iter()
        .filter_map(|leaf| {
            let path = leaf.path.to_ascii_uppercase();
            if !path.starts_with(&prefix) || !path.ends_with(']') {
                return None;
            }
            let close = path.rfind(']')?;
            if close != path.len() - 1 {
                return None;
            }
            let index = path[prefix.len()..close].parse::<i64>().ok()?;
            Some((index, leaf))
        })
        .min_by_key(|(index, _)| *index)
        .map(|(_, leaf)| leaf)
        .ok_or_else(|| format!("Cannot resolve OPC-UA array address for {base}"))?;
    Ok(json!({
        "node_id": string_or(node, "nodeId", ""),
        "browse_name": string_or(node, "browseName", ""),
        "display_name": string_or(node, "displayName", ""),
        "datatype": leaf.type_name,
        "size": leaf.size,
        "length": integer_or(node, "arrayLength", 1),
        "arr": leaf.array_idx,
        "elem": leaf.elem_idx,
        "permissions": node.get("permissions").cloned().unwrap_or_else(default_permissions),
    }))
}

fn opc_structure(
    node: &Value,
    instances: &[Value],
    debug_map: &DebugMap,
) -> Result<Option<Value>, String> {
    let fields = node
        .get("fields")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if fields.is_empty() {
        let leaf = resolve_opc_leaf(node, instances, debug_map)?;
        return Ok(Some(json!({
            "node_id": string_or(node, "nodeId", ""),
            "browse_name": string_or(node, "browseName", ""),
            "display_name": string_or(node, "displayName", ""),
            "description": string_or(node, "description", ""),
            "fields": [{
                "name": string_or(node, "variablePath", ""),
                "datatype": leaf.type_name,
                "size": leaf.size,
                "arr": leaf.array_idx,
                "elem": leaf.elem_idx,
                "permissions": node.get("permissions").cloned().unwrap_or_else(default_permissions),
            }]
        })));
    }
    let base = opc_base_path(node, instances)?;
    let mut resolved = Vec::new();
    for field in fields {
        if let Some(runtime) = opc_field(&field, &base, debug_map) {
            resolved.push(runtime);
        }
    }
    if resolved.is_empty() {
        return Ok(None);
    }
    Ok(Some(json!({
        "node_id": string_or(node, "nodeId", ""),
        "browse_name": string_or(node, "browseName", ""),
        "display_name": string_or(node, "displayName", ""),
        "description": string_or(node, "description", ""),
        "fields": resolved,
    })))
}

fn opc_field(field: &Value, parent: &str, debug_map: &DebugMap) -> Option<Value> {
    let field_path = string_or(field, "fieldPath", "");
    let full = format!("{parent}.{field_path}").to_ascii_uppercase();
    let nested = field
        .get("fields")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if !nested.is_empty() {
        let children = nested
            .iter()
            .filter_map(|child| opc_field(child, &full, debug_map))
            .collect::<Vec<_>>();
        if children.is_empty() {
            return None;
        }
        return Some(json!({
            "name": field_path,
            "datatype": string_or(field, "datatype", "UNKNOWN"),
            "size": Value::Null,
            "arr": Value::Null,
            "elem": Value::Null,
            "permissions": field.get("permissions").cloned().unwrap_or_else(default_permissions),
            "fields": children,
        }));
    }
    let leaf = debug_map
        .leaves
        .iter()
        .find(|leaf| leaf.path.eq_ignore_ascii_case(&full))?;
    Some(json!({
        "name": field_path,
        "datatype": leaf.type_name,
        "size": leaf.size,
        "arr": leaf.array_idx,
        "elem": leaf.elem_idx,
        "permissions": field.get("permissions").cloned().unwrap_or_else(default_permissions),
    }))
}

fn resolve_opc_leaf<'a>(
    node: &Value,
    instances: &[Value],
    debug_map: &'a DebugMap,
) -> Result<&'a DebugLeaf, String> {
    let path = opc_base_path(node, instances)?;
    debug_map
        .leaves
        .iter()
        .find(|leaf| leaf.path.eq_ignore_ascii_case(&path))
        .ok_or_else(|| format!("Cannot resolve OPC-UA variable address for {path}"))
}

fn opc_base_path(node: &Value, instances: &[Value]) -> Result<String, String> {
    let pou = string_or(node, "pouName", "");
    let variable = string_or(node, "variablePath", "");
    if pou.eq_ignore_ascii_case("GVL") || pou.eq_ignore_ascii_case("CONFIG") {
        return Ok(variable.to_ascii_uppercase());
    }
    let instance = instances
        .iter()
        .find(|instance| {
            instance
                .get("program")
                .and_then(Value::as_str)
                .is_some_and(|program| program.eq_ignore_ascii_case(&pou))
        })
        .and_then(|instance| instance.get("name"))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Cannot find instance for program {pou:?} in Resources."))?;
    Ok(format!(
        "{}.{}",
        instance.to_ascii_uppercase(),
        debug_variable_suffix(&variable)
    ))
}

fn debug_variable_suffix(variable: &str) -> String {
    variable
        .split('.')
        .map(|part| {
            if part.starts_with('[') {
                part.to_owned()
            } else {
                part.to_ascii_uppercase()
            }
        })
        .collect::<Vec<_>>()
        .join(".")
        .replace(".[", "[")
}

fn default_permissions() -> Value {
    json!({ "viewer": "r", "operator": "rw", "engineer": "rw" })
}

fn string_or(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_owned()
}

fn integer_or(value: &Value, key: &str, default: i64) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or(default)
}

fn nested_integer_or(value: Option<&Value>, path: &[&str], default: i64) -> i64 {
    let Some(value) = value else {
        return default;
    };
    path.iter()
        .try_fold(value, |current, key| current.get(*key))
        .and_then(Value::as_i64)
        .unwrap_or(default)
}

fn int_nested(value: &Value, path: &[&str], default: i64) -> i64 {
    path.iter()
        .try_fold(value, |current, key| current.get(*key).ok_or(()))
        .ok()
        .and_then(Value::as_i64)
        .unwrap_or(default)
}

fn bool_nested(value: &Value, path: &[&str], default: bool) -> bool {
    path.iter()
        .try_fold(value, |current, key| current.get(*key).ok_or(()))
        .ok()
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

fn format_modbus_offset(raw: &str) -> String {
    let raw = raw.trim();
    if raw.to_ascii_lowercase().starts_with("0x") {
        return raw.to_owned();
    }
    raw.parse::<u64>()
        .map(|value| format!("0x{value:04X}"))
        .unwrap_or_else(|_| "0x0000".to_owned())
}

fn parse_hex_integer(raw: &str) -> i64 {
    let raw = raw.trim();
    let raw = raw
        .strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .unwrap_or(raw);
    i64::from_str_radix(raw, 16).unwrap_or(0)
}

fn parse_numeric_value(raw: &str) -> Value {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("true") {
        return Value::from(1);
    }
    if raw.eq_ignore_ascii_case("false") {
        return Value::from(0);
    }
    let hex = raw
        .strip_prefix("0x")
        .or_else(|| raw.strip_prefix("0X"))
        .or_else(|| raw.strip_prefix("#x"))
        .or_else(|| raw.strip_prefix("#X"));
    if let Some(hex) = hex {
        return Value::from(i64::from_str_radix(hex, 16).unwrap_or(0));
    }
    if let Ok(integer) = raw.parse::<i64>() {
        return Value::from(integer);
    }
    Value::from(raw.parse::<f64>().unwrap_or(0.0))
}

fn pretty(value: Value) -> Result<String, String> {
    serde_json::to_string_pretty(&value).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use open_rust_plc_compiler::backend::{DebugArray, DebugLeaf, DebugMap};
    use serde_json::{json, Value};

    use super::{default_remote_device, default_server, generate_runtime_confs, RuntimeConfs};
    use crate::project::{OpenProject, ProjectDocument, ProjectKind, ProjectMeta};

    fn project(data: Value) -> OpenProject {
        OpenProject {
            root: PathBuf::from("/tmp/protocols"),
            project: ProjectDocument {
                meta: ProjectMeta {
                    name: "Protocols".to_owned(),
                    kind: ProjectKind::PlcProject,
                    extra: Default::default(),
                },
                data,
                extra: Default::default(),
            },
            pous: Vec::new(),
            warnings: Vec::new(),
            device_configuration: json!({}),
            device_pin_mapping: json!({}),
            library_manifest: None,
        }
    }

    #[test]
    fn server_and_remote_defaults_match_pinned_editor() {
        let modbus = default_server("M", "modbus-tcp").unwrap();
        assert_eq!(modbus["modbusSlaveConfig"]["port"], 502);
        let s7 = default_server("S", "s7comm").unwrap();
        assert_eq!(s7["s7commSlaveConfig"]["server"]["pduSize"], 480);
        let opc = default_server("O", "opcua").unwrap();
        assert_eq!(opc["opcuaServerConfig"]["server"]["port"], 4840);
        let remote = default_remote_device("R", "modbus-tcp").unwrap();
        assert_eq!(remote["modbusTcpConfig"]["host"], "127.0.0.1");
        assert_eq!(remote["modbusTcpConfig"]["timeout"], 1000);
    }

    #[test]
    fn runtime_generators_match_modbus_s7_and_ethercat_contracts() {
        let data = json!({
            "configuration":{"resource":{"instances":[],"tasks":[],"globalVariables":[]}},
            "servers":[
                {
                    "name":"mb","protocol":"modbus-tcp",
                    "modbusSlaveConfig":{"enabled":true,"networkInterface":"127.0.0.1","port":1502}
                },
                {
                    "name":"s7","protocol":"s7comm",
                    "s7commSlaveConfig": default_server("s7","s7comm").unwrap()["s7commSlaveConfig"].clone()
                }
            ],
            "remoteDevices":[
                {
                    "name":"sensor","protocol":"modbus-tcp",
                    "modbusTcpConfig":{
                        "host":"10.0.0.5","port":502,"timeout":750,"slaveId":7,
                        "ioGroups":[{"functionCode":"3","cycleTime":100,"offset":"16","length":2,"errorHandling":"set-to-zero","ioPoints":[{"iecLocation":"%IW0"}]}]
                    }
                },
                {
                    "name":"ec","protocol":"ethercat",
                    "ethercatConfig":{
                        "masterConfig":{"enabled":true,"networkInterface":"en0","cycleTimeUs":1000},
                        "devices":[{
                            "name":"slave","vendorId":"0x1","productCode":"0x2","revisionNo":"0x3","slaveType":"coupler",
                            "config":{
                                "startupChecks":{"checkVendorId":true,"checkProductCode":true},
                                "addressing":{"ethercatAddress":1},
                                "timeouts":{"sdoTimeoutMs":1000,"initToPreOpTimeoutMs":5000,"safeOpToOpTimeoutMs":5000},
                                "watchdog":{"smWatchdogEnabled":false,"smWatchdogMs":100,"pdiWatchdogEnabled":false,"pdiWatchdogMs":100},
                                "distributedClocks":{"dcEnabled":false,"dcSyncUnitCycleUs":1000,"dcSync0Enabled":false,"dcSync0CycleUs":1000,"dcSync0ShiftUs":0,"dcSync1Enabled":false,"dcSync1CycleUs":1000,"dcSync1ShiftUs":0}
                            },
                            "channelMappings":[],"channelInfo":[]
                        }]
                    }
                }
            ]
        });
        let confs = generate_runtime_confs(
            &project(data),
            &DebugMap {
                version: 2,
                md5: String::new(),
                type_tags: Default::default(),
                arrays: Vec::new(),
                leaves: Vec::new(),
            },
        )
        .unwrap();
        let slave: Value = serde_json::from_str(confs.modbus_slave.as_deref().unwrap()).unwrap();
        assert_eq!(slave["network_configuration"]["port"], 1502);
        assert_eq!(
            slave["buffer_mapping"]["holding_registers"]["qw_count"],
            1024
        );
        let master: Value = serde_json::from_str(confs.modbus_master.as_deref().unwrap()).unwrap();
        assert_eq!(master[0]["config"]["io_points"][0]["offset"], "0x0010");
        assert_eq!(
            master[0]["config"]["io_points"][0]["error_handling"],
            "set-to-zero"
        );
        let s7: Value = serde_json::from_str(confs.s7comm.as_deref().unwrap()).unwrap();
        assert_eq!(s7["server"]["port"], 102);
        let ethercat: Value = serde_json::from_str(confs.ethercat.as_deref().unwrap()).unwrap();
        assert_eq!(ethercat[0]["config"]["master"]["interface"], "en0");
    }

    #[test]
    fn opcua_runtime_addresses_come_from_debug_map() {
        let data = json!({
            "configuration":{"resource":{"instances":[{"name":"instance0","program":"main","task":"t"}],"tasks":[],"globalVariables":[]}},
            "servers":[{
                "name":"opc","protocol":"opcua",
                "opcuaServerConfig":{
                    "server":{"enabled":true,"name":"OpenPLC OPC UA Server","applicationUri":"urn:a","productUri":"urn:p","bindAddress":"0.0.0.0","port":4840,"endpointPath":"/openplc/opcua"},
                    "securityProfiles":[{"name":"insecure","enabled":true,"securityPolicy":"None","securityMode":"None","authMethods":["Anonymous"]}],
                    "security":{"serverCertificateStrategy":"auto_self_signed","serverCertificateCustom":null,"serverPrivateKeyCustom":null,"trustedClientCertificates":[]},
                    "users":[],"cycleTimeMs":100,
                    "addressSpace":{"namespaceUri":"urn:n","nodes":[{
                        "pouName":"main","variablePath":"x","nodeType":"variable","nodeId":"ns=2;s=x","browseName":"x","displayName":"X","description":"","permissions":{"viewer":"r","operator":"rw","engineer":"rw"}
                    }]}
                }
            }],
            "remoteDevices":[]
        });
        let debug_map = DebugMap {
            version: 2,
            md5: String::new(),
            type_tags: Default::default(),
            arrays: vec![DebugArray { index: 0, count: 1 }],
            leaves: vec![DebugLeaf {
                array_idx: 0,
                elem_idx: 0,
                path: "INSTANCE0.X".to_owned(),
                type_name: "INT".to_owned(),
                size: 2,
            }],
        };
        let confs: RuntimeConfs = generate_runtime_confs(&project(data), &debug_map).unwrap();
        let opc: Value = serde_json::from_str(confs.opcua.as_deref().unwrap()).unwrap();
        assert_eq!(opc[0]["config"]["format_version"], 2);
        assert_eq!(opc[0]["config"]["address_space"]["variables"][0]["arr"], 0);
        assert_eq!(
            opc[0]["config"]["address_space"]["variables"][0]["datatype"],
            "INT"
        );
        assert_eq!(opc[0]["config"]["address_space"]["variables"][0]["size"], 2);
    }
}
