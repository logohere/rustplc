use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StlibArchive {
    pub format_version: u8,
    pub manifest: LibraryManifest,
    #[serde(default)]
    pub chunks: Vec<LibraryChunk>,
    #[serde(default)]
    pub sources: Vec<LibrarySource>,
    #[serde(default)]
    pub global_constants: BTreeMap<String, f64>,
    #[serde(default)]
    pub dependencies: Vec<LibraryDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryManifest {
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    pub namespace: String,
    #[serde(default)]
    pub functions: Vec<LibraryFunctionEntry>,
    #[serde(default)]
    pub function_blocks: Vec<LibraryFbEntry>,
    #[serde(default)]
    pub types: Vec<LibraryTypeEntry>,
    #[serde(default)]
    pub globals: Vec<LibraryGlobalEntry>,
    #[serde(default)]
    pub headers: Vec<String>,
    #[serde(default)]
    pub is_builtin: bool,
    #[serde(default)]
    pub source_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFunctionEntry {
    pub name: String,
    pub return_type: String,
    #[serde(default)]
    pub parameters: Vec<LibraryFunctionParameter>,
    #[serde(default)]
    pub variadic: Option<LibraryVariadic>,
    #[serde(default)]
    pub documentation: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFunctionParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub direction: String,
    #[serde(default)]
    pub initial_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryVariadic {
    pub min_args: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryVarType {
    pub name: String,
    #[serde(rename = "type", default)]
    pub type_kind: String,
    #[serde(default)]
    pub array_dimensions: Vec<LibraryArrayDimension>,
    #[serde(default)]
    pub element_type_name: Option<String>,
    #[serde(default)]
    pub reference_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LibraryArrayDimension {
    pub start: i64,
    pub end: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFbEntry {
    pub name: String,
    #[serde(default)]
    pub inputs: Vec<LibraryVarType>,
    #[serde(default)]
    pub outputs: Vec<LibraryVarType>,
    #[serde(default)]
    pub inouts: Vec<LibraryVarType>,
    #[serde(default)]
    pub documentation: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryTypeEntry {
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub base_type: Option<String>,
    #[serde(default)]
    pub fields: Vec<LibraryTypeField>,
    #[serde(default)]
    pub documentation: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LibraryTypeField {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LibraryGlobalEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default)]
    pub constant: bool,
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LibraryChunk {
    pub name: String,
    pub kind: String,
    pub header: String,
    pub cpp: String,
    #[serde(default)]
    pub deps: Vec<LibraryChunkDep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LibraryChunkDep {
    pub library: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySource {
    pub file_name: String,
    pub source: String,
    #[serde(default)]
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LibraryDependency {
    pub name: String,
    pub version: String,
}

impl StlibArchive {
    pub fn parse(raw: &str, source_label: &str) -> Result<Self, String> {
        let archive: Self = serde_json::from_str(raw)
            .map_err(|error| format!("Invalid JSON in stlib archive {source_label}: {error}"))?;
        archive.validate(source_label)?;
        Ok(archive)
    }

    pub fn validate(&self, source_label: &str) -> Result<(), String> {
        if self.format_version != 1 {
            return Err(format!(
                "Invalid stlib archive {source_label}: formatVersion must be 1"
            ));
        }
        if self.manifest.name.trim().is_empty() {
            return Err(format!(
                "Invalid stlib archive {source_label}: manifest.name must be non-empty"
            ));
        }
        if self.manifest.version.trim().is_empty() {
            return Err(format!(
                "Invalid stlib archive {source_label}: manifest.version must be non-empty"
            ));
        }
        let mut chunk_names = HashSet::new();
        for chunk in &self.chunks {
            let name = iec_ident(&chunk.name);
            if name.is_empty() || !chunk_names.insert(name.clone()) {
                return Err(format!(
                    "Invalid stlib archive {source_label}: duplicate/empty chunk name {name:?}"
                ));
            }
            if !matches!(
                chunk.kind.as_str(),
                "function" | "functionBlock" | "type" | "inlineGlobal"
            ) {
                return Err(format!(
                    "Invalid stlib archive {source_label}: unsupported chunk kind {:?}",
                    chunk.kind
                ));
            }
        }
        Ok(())
    }
}

/// Conservative function-level tree-shake equivalent to pinned STruC++.
/// Seeds are symbols referenced in the user's generated ST; dependency edges
/// are then followed across enabled archives. False-positive textual seeds can
/// only retain an extra chunk, never omit a required dependency.
pub fn reachable_chunks<'a>(
    archives: &'a [StlibArchive],
    program_st: &str,
) -> Vec<(&'a StlibArchive, &'a LibraryChunk)> {
    let upper_source = program_st.to_ascii_uppercase();
    let mut lookup = HashMap::<(String, String), (&StlibArchive, &LibraryChunk)>::new();
    for archive in archives {
        for chunk in &archive.chunks {
            lookup.insert(
                (archive.manifest.name.clone(), iec_ident(&chunk.name)),
                (archive, chunk),
            );
        }
    }

    let mut queue = VecDeque::<(String, String)>::new();
    let mut seen = HashSet::<(String, String)>::new();
    for archive in archives {
        for chunk in &archive.chunks {
            let symbol = iec_ident(&chunk.name);
            if source_has_identifier(&upper_source, &symbol) {
                queue.push_back((archive.manifest.name.clone(), symbol));
            }
        }
    }

    while let Some(key) = queue.pop_front() {
        if !seen.insert(key.clone()) {
            continue;
        }
        let Some((archive, chunk)) = lookup.get(&key).copied() else {
            continue;
        };
        for dep in &chunk.deps {
            let library = if dep.library == "this" {
                archive.manifest.name.clone()
            } else {
                dep.library.clone()
            };
            queue.push_back((library, iec_ident(&dep.name)));
        }
    }

    let mut result = Vec::new();
    for archive in archives {
        for chunk in &archive.chunks {
            if seen.contains(&(archive.manifest.name.clone(), iec_ident(&chunk.name))) {
                result.push((archive, chunk));
            }
        }
    }
    result
}

fn iec_ident(value: &str) -> String {
    value.to_ascii_uppercase()
}

fn source_has_identifier(source: &str, symbol: &str) -> bool {
    let bytes = source.as_bytes();
    let needle = symbol.as_bytes();
    if needle.is_empty() || needle.len() > bytes.len() {
        return false;
    }
    bytes
        .windows(needle.len())
        .enumerate()
        .any(|(index, window)| {
            window == needle
                && (index == 0 || !is_ident_byte(bytes[index - 1]))
                && (index + needle.len() == bytes.len()
                    || !is_ident_byte(bytes[index + needle.len()]))
        })
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
mod tests {
    use super::{reachable_chunks, StlibArchive};

    #[test]
    fn parses_and_tree_shakes_chunk_dependencies() {
        let raw = r#"{
          "formatVersion":1,
          "manifest":{"name":"demo","version":"1.0.0","namespace":"demo","functions":[],"functionBlocks":[],"types":[],"headers":[],"isBuiltin":false},
          "chunks":[
            {"name":"A","kind":"function","header":"A","cpp":"A","deps":[{"library":"demo","name":"B"}]},
            {"name":"B","kind":"function","header":"B","cpp":"B","deps":[]},
            {"name":"UNUSED","kind":"function","header":"U","cpp":"U","deps":[]}
          ],
          "dependencies":[]
        }"#;
        let archive = StlibArchive::parse(raw, "demo.stlib").unwrap();
        let archives = [archive];
        let reachable = reachable_chunks(&archives, "PROGRAM main\nA();\nEND_PROGRAM");
        assert_eq!(
            reachable
                .iter()
                .map(|(_, chunk)| chunk.name.as_str())
                .collect::<Vec<_>>(),
            vec!["A", "B"]
        );
    }
}
