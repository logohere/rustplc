use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use crate::frontend::ast::{
    CompilationUnit, FunctionBlockDeclaration, ReferenceKind, TypeDefinition, TypeReference,
    VarBlockType,
};

use super::names::iec_ident;
use super::type_registry::TypeRegistry;

const MAX_ENTRIES_PER_ARRAY: usize = 8_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DebugMap {
    pub version: u8,
    pub md5: String,
    #[serde(rename = "typeTags")]
    pub type_tags: BTreeMap<String, u8>,
    pub arrays: Vec<DebugArray>,
    pub leaves: Vec<DebugLeaf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DebugArray {
    pub index: usize,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DebugLeaf {
    #[serde(rename = "arrayIdx")]
    pub array_idx: usize,
    #[serde(rename = "elemIdx")]
    pub elem_idx: usize,
    pub path: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub size: usize,
}

#[derive(Debug)]
struct DebugEntry {
    cpp_expr: String,
    tag: &'static str,
    size: usize,
    path: String,
}

struct DebugBuilder<'a> {
    ast: &'a CompilationUnit,
    types: &'a TypeRegistry,
    arrays: Vec<Vec<DebugEntry>>,
}

pub fn generate_debug_artifacts(
    ast: &CompilationUnit,
    types: &TypeRegistry,
) -> Result<(String, DebugMap), String> {
    let mut builder = DebugBuilder {
        ast,
        types,
        arrays: vec![Vec::new()],
    };
    builder.walk_globals()?;
    builder.walk_program_instances()?;
    builder.finish();
    Ok((builder.render(), builder.debug_map()))
}

impl DebugBuilder<'_> {
    fn tail(&mut self) -> &mut Vec<DebugEntry> {
        self.arrays.last_mut().expect("debug table has a bucket")
    }

    fn ensure_room(&mut self) {
        if self
            .arrays
            .last()
            .is_some_and(|array| array.len() >= MAX_ENTRIES_PER_ARRAY)
        {
            self.arrays.push(Vec::new());
        }
    }

    fn add_leaf(&mut self, path: String, cpp_expr: String, type_name: &str) {
        let Some(tag) = elementary_tag(type_name) else {
            return;
        };
        self.ensure_room();
        self.tail().push(DebugEntry {
            cpp_expr,
            tag,
            size: elementary_size(type_name),
            path,
        });
    }

    fn walk_globals(&mut self) -> Result<(), String> {
        let mut seen = HashSet::new();
        for config in &self.ast.configurations {
            for block in &config.var_blocks {
                if block.block_type != VarBlockType::Global {
                    continue;
                }
                for declaration in &block.declarations {
                    for variable in &declaration.names {
                        let name = iec_ident(variable);
                        if !seen.insert(name.clone()) {
                            continue;
                        }
                        self.visit_type_ref(
                            name.clone(),
                            format!("{name}.value"),
                            &declaration.variable_type,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    fn walk_program_instances(&mut self) -> Result<(), String> {
        for config in &self.ast.configurations {
            let config_name = iec_ident(&config.name);
            let _ = config_name;
            for resource in &config.resources {
                for task in &resource.tasks {
                    for instance in resource.program_instances.iter().filter(|instance| {
                        instance
                            .task_name
                            .as_deref()
                            .is_some_and(|name| iec_ident(name) == iec_ident(&task.name))
                    }) {
                        if self.arrays.last().is_some_and(|array| !array.is_empty()) {
                            self.arrays.push(Vec::new());
                        }
                        let Some(program) = self.ast.programs.iter().find(|program| {
                            iec_ident(&program.name) == iec_ident(&instance.program_type)
                        }) else {
                            continue;
                        };
                        let instance_name = iec_ident(&instance.instance_name);
                        for block in &program.var_blocks {
                            if !matches!(
                                block.block_type,
                                VarBlockType::Var | VarBlockType::Input | VarBlockType::Output
                            ) {
                                continue;
                            }
                            for declaration in &block.declarations {
                                for variable in &declaration.names {
                                    let variable_name = iec_ident(variable);
                                    let cpp_name = self
                                        .types
                                        .member_cpp_name(variable, &declaration.variable_type.name);
                                    self.visit_type_ref(
                                        format!("{instance_name}.{variable_name}"),
                                        format!("g_config.{instance_name}.{cpp_name}"),
                                        &declaration.variable_type,
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn visit_type_ref(
        &mut self,
        path: String,
        cpp_expr: String,
        type_ref: &TypeReference,
    ) -> Result<(), String> {
        if type_ref.reference_kind != ReferenceKind::None {
            return Ok(());
        }
        if !type_ref.array_dimensions.is_empty() {
            let Some(element) = type_ref.element_type_name.as_deref() else {
                return Ok(());
            };
            let bounds = constant_bounds(&type_ref.array_dimensions, self.types)?;
            return self.walk_array(&path, &cpp_expr, &bounds, 0, element);
        }

        let name = iec_ident(&type_ref.name);
        if elementary_tag(&name).is_some() {
            self.add_leaf(path, cpp_expr, &name);
            return Ok(());
        }

        if let Some(declaration) = self
            .ast
            .types
            .iter()
            .find(|declaration| iec_ident(&declaration.name) == name)
        {
            match &declaration.definition {
                TypeDefinition::Struct(fields) => {
                    for field in fields {
                        for field_name in &field.names {
                            let field_path = iec_ident(field_name);
                            let field_cpp = self
                                .types
                                .member_cpp_name(field_name, &field.variable_type.name);
                            self.visit_type_ref(
                                format!("{path}.{field_path}"),
                                format!("{cpp_expr}.{field_cpp}"),
                                &field.variable_type,
                            )?;
                        }
                    }
                }
                TypeDefinition::Array {
                    dimensions,
                    element_type,
                } => {
                    let bounds = constant_bounds(dimensions, self.types)?;
                    self.walk_array(&path, &cpp_expr, &bounds, 0, &element_type.name)?;
                }
                TypeDefinition::Enum { base_type, .. } => {
                    let base = base_type
                        .as_ref()
                        .map(|base| base.name.as_str())
                        .unwrap_or("INT");
                    self.add_leaf(path, cpp_expr, base);
                }
                TypeDefinition::Subrange { base_type, .. } => {
                    self.visit_type_ref(path, cpp_expr, base_type)?;
                }
                TypeDefinition::Alias(base_type) => {
                    self.visit_type_ref(path, cpp_expr, base_type)?;
                }
            }
            return Ok(());
        }

        if let Some(function_block) = self
            .ast
            .function_blocks
            .iter()
            .find(|function_block| iec_ident(&function_block.name) == name)
        {
            self.visit_function_block(&path, &cpp_expr, function_block)?;
        }
        Ok(())
    }

    fn visit_function_block(
        &mut self,
        path: &str,
        cpp_expr: &str,
        function_block: &FunctionBlockDeclaration,
    ) -> Result<(), String> {
        let bundled_library = function_block.span.file.starts_with("iec-standard-fb/");
        if !bundled_library {
            if let Some(parent_name) = function_block.extends.as_deref() {
                if let Some(parent) = self
                    .ast
                    .function_blocks
                    .iter()
                    .find(|candidate| iec_ident(&candidate.name) == iec_ident(parent_name))
                {
                    self.visit_function_block(path, cpp_expr, parent)?;
                }
            }
        }
        for block in &function_block.var_blocks {
            let include = if bundled_library {
                matches!(
                    block.block_type,
                    VarBlockType::Input | VarBlockType::Output | VarBlockType::InOut
                )
            } else {
                matches!(
                    block.block_type,
                    VarBlockType::Var
                        | VarBlockType::Input
                        | VarBlockType::Output
                        | VarBlockType::InOut
                        | VarBlockType::Inst
                )
            };
            if !include {
                continue;
            }
            for declaration in &block.declarations {
                for variable in &declaration.names {
                    let variable_name = iec_ident(variable);
                    let cpp_name = self
                        .types
                        .member_cpp_name(variable, &declaration.variable_type.name);
                    self.visit_type_ref(
                        format!("{path}.{variable_name}"),
                        format!("{cpp_expr}.{cpp_name}"),
                        &declaration.variable_type,
                    )?;
                }
            }
        }
        Ok(())
    }

    fn walk_array(
        &mut self,
        path: &str,
        cpp_expr: &str,
        bounds: &[(i64, i64)],
        dimension: usize,
        element_type: &str,
    ) -> Result<(), String> {
        if dimension == bounds.len() {
            let type_ref = TypeReference {
                name: element_type.to_owned(),
                reference_kind: crate::frontend::ast::ReferenceKind::None,
                max_length: None,
                array_dimensions: Vec::new(),
                element_type_name: None,
                span: crate::frontend::SourceSpan::new("<debug>", 0, 0, 0, 0),
            };
            return self.visit_type_ref(path.to_owned(), cpp_expr.to_owned(), &type_ref);
        }
        let (start, end) = bounds[dimension];
        for index in start..=end {
            self.walk_array(
                &format!("{path}[{index}]"),
                &format!("{cpp_expr}[{index}]"),
                bounds,
                dimension + 1,
                element_type,
            )?;
        }
        Ok(())
    }

    fn finish(&mut self) {
        if self.arrays.len() > 1 && self.arrays.last().is_some_and(Vec::is_empty) {
            self.arrays.pop();
        }
        if self.arrays.is_empty() {
            self.arrays.push(Vec::new());
        }
    }

    fn debug_map(&self) -> DebugMap {
        let arrays = self
            .arrays
            .iter()
            .enumerate()
            .map(|(index, array)| DebugArray {
                index,
                count: array.len(),
            })
            .collect();
        let leaves = self
            .arrays
            .iter()
            .enumerate()
            .flat_map(|(array_idx, array)| {
                array
                    .iter()
                    .enumerate()
                    .map(move |(elem_idx, entry)| DebugLeaf {
                        array_idx,
                        elem_idx,
                        path: entry.path.clone(),
                        type_name: entry.tag.to_owned(),
                        size: entry.size,
                    })
            })
            .collect();
        DebugMap {
            version: 2,
            md5: String::new(),
            type_tags: type_tags(),
            arrays,
            leaves,
        }
    }

    fn render(&self) -> String {
        let config_name = self
            .ast
            .configurations
            .first()
            .map(|config| iec_ident(&config.name))
            .unwrap_or_else(|| "CONFIG0".to_owned());
        let mut out = String::new();
        line(&mut out, "// SPDX-License-Identifier: GPL-3.0-or-later");
        line(
            &mut out,
            "// Generated by STruC++ debug-table-gen - Do not edit by hand.",
        );
        line(&mut out, "//");
        line(
            &mut out,
            "// Per-project debugger pointer tables consumed by",
        );
        line(
            &mut out,
            "// strucpp::debug::handle_*() in debug_dispatch.hpp.",
        );
        line(&mut out, "");
        line(&mut out, "#include \"generated.hpp\"");
        line(&mut out, "#include \"debug_table.hpp\"");
        line(&mut out, "");
        line(
            &mut out,
            "// The sketch/runtime must define this global with external linkage:",
        );
        line(
            &mut out,
            &format!("//   strucpp::Configuration_{config_name} g_config;"),
        );
        line(
            &mut out,
            "// The debug table below reaches into it via compile-time",
        );
        line(
            &mut out,
            "// address-of expressions — so it must be a real object, not a",
        );
        line(&mut out, "// static-local or a pointer.");
        line(
            &mut out,
            &format!("extern ::strucpp::Configuration_{config_name} g_config;"),
        );
        line(&mut out, "");
        line(&mut out, "namespace strucpp { namespace debug {");
        line(&mut out, "");
        for (index, array) in self.arrays.iter().enumerate() {
            line(
                &mut out,
                &format!(
                    "const Entry debug_arr_{index}[{}] STRUCPP_DEBUG_FLASH = {{",
                    array.len().max(1)
                ),
            );
            if array.is_empty() {
                line(
                    &mut out,
                    "    { nullptr, 0, 0 },  // placeholder — array is empty",
                );
            } else {
                for entry in array {
                    line(
                        &mut out,
                        &format!(
                            "    {{ (void*)&{}, TAG_{}, 0 }},  // {}",
                            entry.cpp_expr, entry.tag, entry.path
                        ),
                    );
                }
            }
            line(&mut out, "};");
            line(&mut out, "");
        }
        line(
            &mut out,
            &format!(
                "const Entry* const debug_arrays[{}] STRUCPP_DEBUG_FLASH = {{",
                self.arrays.len()
            ),
        );
        for index in 0..self.arrays.len() {
            line(&mut out, &format!("    debug_arr_{index},"));
        }
        line(&mut out, "};");
        line(&mut out, "");
        line(
            &mut out,
            &format!(
                "const uint16_t debug_array_counts[{}] STRUCPP_DEBUG_FLASH = {{",
                self.arrays.len()
            ),
        );
        for array in &self.arrays {
            line(&mut out, &format!("    {},", array.len()));
        }
        line(&mut out, "};");
        line(&mut out, "");
        line(
            &mut out,
            &format!("const uint8_t debug_array_count = {};", self.arrays.len()),
        );
        line(&mut out, "");
        line(&mut out, "} } // namespace strucpp::debug");
        out
    }
}

fn constant_bounds(
    dimensions: &[crate::frontend::ast::ArrayDimension],
    types: &TypeRegistry,
) -> Result<Vec<(i64, i64)>, String> {
    dimensions
        .iter()
        .map(|dimension| {
            if dimension.is_variable_length {
                return Err(
                    "variable-length persistent arrays cannot be enumerated for debug".to_owned(),
                );
            }
            let start = types
                .constant_i64(dimension.start.as_ref())
                .ok_or_else(|| "debug array lower bound must be constant".to_owned())?;
            let end = types
                .constant_i64(dimension.end.as_ref())
                .ok_or_else(|| "debug array upper bound must be constant".to_owned())?;
            Ok((start, end))
        })
        .collect()
}

fn type_tags() -> BTreeMap<String, u8> {
    [
        ("BOOL", 0),
        ("SINT", 1),
        ("USINT", 2),
        ("INT", 3),
        ("UINT", 4),
        ("DINT", 5),
        ("UDINT", 6),
        ("LINT", 7),
        ("ULINT", 8),
        ("REAL", 9),
        ("LREAL", 10),
        ("BYTE", 11),
        ("WORD", 12),
        ("DWORD", 13),
        ("LWORD", 14),
        ("TIME", 15),
        ("DATE", 16),
        ("TOD", 17),
        ("DT", 18),
        ("STRING", 19),
        ("WSTRING", 20),
    ]
    .into_iter()
    .map(|(name, tag)| (name.to_owned(), tag))
    .collect()
}

fn elementary_size(name: &str) -> usize {
    match iec_ident(name).as_str() {
        "BOOL" | "SINT" | "USINT" | "BYTE" => 1,
        "INT" | "UINT" | "WORD" => 2,
        "DINT" | "UDINT" | "REAL" | "DWORD" => 4,
        "LINT" | "ULINT" | "LREAL" | "LWORD" | "__XWORD" | "TIME" | "LTIME" | "DATE" | "LDATE"
        | "TOD" | "TIME_OF_DAY" | "LTOD" | "DT" | "DATE_AND_TIME" | "LDT" => 8,
        "STRING" => 127,
        "WSTRING" => 253,
        _ => 0,
    }
}

fn elementary_tag(name: &str) -> Option<&'static str> {
    Some(match iec_ident(name).as_str() {
        "BOOL" => "BOOL",
        "SINT" => "SINT",
        "USINT" => "USINT",
        "INT" => "INT",
        "UINT" => "UINT",
        "DINT" => "DINT",
        "UDINT" => "UDINT",
        "LINT" => "LINT",
        "ULINT" => "ULINT",
        "REAL" => "REAL",
        "LREAL" => "LREAL",
        "BYTE" => "BYTE",
        "WORD" => "WORD",
        "DWORD" => "DWORD",
        "LWORD" | "__XWORD" => "LWORD",
        "TIME" | "LTIME" => "TIME",
        "DATE" | "LDATE" => "DATE",
        "TOD" | "TIME_OF_DAY" | "LTOD" => "TOD",
        "DT" | "DATE_AND_TIME" | "LDT" => "DT",
        "STRING" => "STRING",
        "WSTRING" => "WSTRING",
        _ => return None,
    })
}

fn line(out: &mut String, value: &str) {
    out.push_str(value);
    out.push('\n');
}
