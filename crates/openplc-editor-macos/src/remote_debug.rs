use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use open_rust_plc_compiler::backend::DebugMap;

use crate::debug_values::{
    decode_debug_value, encode_force_value, watched_debug_variables, DebugValue, DebugVariable,
};
use crate::debug_wire::{pack_debug_addr, TargetEndian};
use crate::project::OpenProject;
use crate::runtime_debug_ws::RuntimeDebugTransport;

const WEBSOCKET_BATCH_SIZE: usize = 500;
const MIN_BATCH_SIZE: usize = 2;

#[derive(Clone)]
pub(crate) struct RemoteDebugSession {
    transport: RuntimeDebugTransport,
    debug_map: DebugMap,
    target_endian: TargetEndian,
    forced_paths: Arc<Mutex<HashSet<String>>>,
}

impl RemoteDebugSession {
    pub(crate) fn connect(
        host: &str,
        token: &str,
        debug_map: DebugMap,
        expected_md5: &str,
        allow_insecure_tls: bool,
    ) -> Result<Self, String> {
        let transport = RuntimeDebugTransport::connect(host, 8443, token, allow_insecure_tls)?;
        let probe = match transport.get_md5() {
            Ok(probe) => probe,
            Err(error) => {
                let _ = transport.disconnect();
                return Err(error);
            }
        };
        if !probe.md5.eq_ignore_ascii_case(expected_md5) {
            let _ = transport.disconnect();
            return Err(format!(
                "Runtime program MD5 {} does not match the current project {}. Deploy the project before starting remote debug.",
                probe.md5, expected_md5
            ));
        }
        Ok(Self {
            transport,
            debug_map,
            target_endian: probe.target_endian,
            forced_paths: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    pub(crate) fn disconnect(&self) -> Result<(), String> {
        self.transport.disconnect()
    }

    pub(crate) fn debug_values(&self, project: &OpenProject) -> Result<Vec<DebugValue>, String> {
        let forced = self
            .forced_paths
            .lock()
            .map_err(|_| "remote debug force-state lock poisoned".to_owned())?
            .clone();
        let variables = watched_debug_variables(project, &self.debug_map, &forced)?;
        self.read_variables(&variables)
    }

    pub(crate) fn force_debug_value(
        &self,
        project: &OpenProject,
        path: &str,
        input: &str,
    ) -> Result<(), String> {
        let variable = self.debug_variable(project, path)?;
        let mut bytes = encode_force_value(&variable.type_name, input)?;
        if bytes.len() != variable.size {
            return Err(format!(
                "debug force value for {} encoded to {} bytes; runtime requires {}",
                variable.path,
                bytes.len(),
                variable.size
            ));
        }
        normalize_numeric_endian(&mut bytes, &variable.type_name, self.target_endian);
        self.transport.set_variable(
            pack_debug_addr(variable.array, variable.element),
            true,
            Some(&bytes),
        )?;
        self.forced_paths
            .lock()
            .map_err(|_| "remote debug force-state lock poisoned".to_owned())?
            .insert(variable.path);
        Ok(())
    }

    pub(crate) fn unforce_debug_value(
        &self,
        project: &OpenProject,
        path: &str,
    ) -> Result<(), String> {
        let variable = self.debug_variable(project, path)?;
        self.transport.set_variable(
            pack_debug_addr(variable.array, variable.element),
            false,
            None,
        )?;
        self.forced_paths
            .lock()
            .map_err(|_| "remote debug force-state lock poisoned".to_owned())?
            .remove(&variable.path);
        Ok(())
    }

    fn debug_variable(&self, project: &OpenProject, path: &str) -> Result<DebugVariable, String> {
        let forced = self
            .forced_paths
            .lock()
            .map_err(|_| "remote debug force-state lock poisoned".to_owned())?
            .clone();
        watched_debug_variables(project, &self.debug_map, &forced)?
            .into_iter()
            .find(|variable| variable.path.eq_ignore_ascii_case(path))
            .ok_or_else(|| format!("Unknown watched debug variable path: {path}"))
    }

    fn read_variables(&self, variables: &[DebugVariable]) -> Result<Vec<DebugValue>, String> {
        let mut values = Vec::with_capacity(variables.len());
        let mut offset = 0usize;
        let mut batch_size = WEBSOCKET_BATCH_SIZE.min(variables.len().max(1));

        while offset < variables.len() {
            let end = (offset + batch_size).min(variables.len());
            let batch = &variables[offset..end];
            let indexes = batch
                .iter()
                .map(|variable| pack_debug_addr(variable.array, variable.element))
                .collect::<Vec<_>>();
            let response = match self.transport.get_variables_list(&indexes) {
                Ok(response) => response,
                Err(error)
                    if error.contains("ERROR_OUT_OF_MEMORY") && batch_size > MIN_BATCH_SIZE =>
                {
                    batch_size = (batch_size / 2).max(MIN_BATCH_SIZE);
                    continue;
                }
                Err(error) => return Err(error),
            };

            let runtime_count = (usize::from(response.last_index) + 1).min(batch.len());
            let mut buffer_offset = 0usize;
            let mut consumed = 0usize;
            for variable in batch.iter().take(runtime_count) {
                if buffer_offset + variable.size > response.data.len() {
                    break;
                }
                let mut bytes =
                    response.data[buffer_offset..buffer_offset + variable.size].to_vec();
                normalize_numeric_endian(&mut bytes, &variable.type_name, self.target_endian);
                let display = decode_debug_value(&variable.type_name, &bytes)?;
                values.push(DebugValue {
                    variable: variable.clone(),
                    display,
                });
                buffer_offset += variable.size;
                consumed += 1;
            }
            if consumed == 0 {
                return Err(
                    "Runtime debug response did not contain any complete watched values."
                        .to_owned(),
                );
            }
            offset += consumed;
        }
        Ok(values)
    }
}

fn normalize_numeric_endian(bytes: &mut [u8], type_name: &str, target: TargetEndian) {
    if target == TargetEndian::Little || bytes.len() <= 1 {
        return;
    }
    let upper = type_name.to_ascii_uppercase();
    if matches!(upper.as_str(), "STRING" | "WSTRING") {
        return;
    }
    bytes.reverse();
}

#[cfg(test)]
mod tests {
    use super::normalize_numeric_endian;
    use crate::debug_wire::TargetEndian;

    #[test]
    fn big_endian_numeric_values_are_normalized_but_strings_are_not() {
        let mut real = vec![0x3f, 0x80, 0x00, 0x00];
        normalize_numeric_endian(&mut real, "REAL", TargetEndian::Big);
        assert_eq!(real, vec![0x00, 0x00, 0x80, 0x3f]);

        let mut string = vec![2, b'h', b'i', 0];
        normalize_numeric_endian(&mut string, "STRING", TargetEndian::Big);
        assert_eq!(string, vec![2, b'h', b'i', 0]);

        let mut little = vec![1, 2, 3, 4];
        normalize_numeric_endian(&mut little, "DINT", TargetEndian::Little);
        assert_eq!(little, vec![1, 2, 3, 4]);
    }
}
