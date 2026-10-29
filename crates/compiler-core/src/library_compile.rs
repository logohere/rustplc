use std::collections::{BTreeMap, HashMap, HashSet};

use crate::backend::{generate_library_items, LibraryCodegenItem};
use crate::frontend::ast::{
    ArrayDimension, BinaryOperator, CompilationUnit, Expression, ReferenceKind, TypeDefinition,
    TypeReference, UnaryOperator, VarBlockType,
};
use crate::frontend::parse;
use crate::library::{
    LibraryArrayDimension, LibraryChunk, LibraryChunkDep, LibraryDependency, LibraryFbEntry,
    LibraryFunctionEntry, LibraryFunctionParameter, LibraryGlobalEntry, LibraryManifest,
    LibrarySource, LibraryTypeEntry, LibraryTypeField, LibraryVarType, StlibArchive,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibrarySourceInput {
    pub file_name: String,
    pub source: String,
    pub category: Option<String>,
    pub documentation: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CompileStlibOptions<'a> {
    pub name: &'a str,
    pub version: &'a str,
    pub namespace: &'a str,
    pub no_source: bool,
    pub builtin: bool,
    pub dependencies: &'a [StlibArchive],
    pub global_constants: BTreeMap<String, f64>,
}

pub fn compile_stlib(
    sources: &[LibrarySourceInput],
    options: &CompileStlibOptions<'_>,
) -> Result<StlibArchive, String> {
    if sources.is_empty() {
        return Err("No source files provided".to_owned());
    }
    if options.name.trim().is_empty()
        || options.version.trim().is_empty()
        || options.namespace.trim().is_empty()
    {
        return Err("library name, version, and namespace must be non-empty".to_owned());
    }

    let mut ast = CompilationUnit::default();
    let mut category_by_name = HashMap::new();
    let mut documentation_by_name = HashMap::new();
    for source in sources {
        let unit = parse(&source.source, &source.file_name).map_err(|errors| {
            errors
                .into_iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        tag_source_metadata(
            &unit,
            source.category.as_deref(),
            source.documentation.as_deref(),
            &mut category_by_name,
            &mut documentation_by_name,
        );
        ast.extend(unit);
    }

    let items = generate_library_items(&ast, options.dependencies, &options.global_constants)
        .map_err(|error| error.to_string())?;
    let ownership = symbol_ownership(&items, options.dependencies, options.name);
    let chunks = items
        .iter()
        .map(|item| LibraryChunk {
            name: item.name.clone(),
            kind: item.kind.to_owned(),
            header: item.header.clone(),
            cpp: item.cpp.clone(),
            deps: infer_dependencies(item, &ownership, options.name),
        })
        .collect::<Vec<_>>();

    let manifest = manifest_from_ast(
        &ast,
        options,
        sources,
        &category_by_name,
        &documentation_by_name,
    );
    let archive = StlibArchive {
        format_version: 1,
        manifest,
        chunks,
        sources: if options.no_source {
            Vec::new()
        } else {
            sources
                .iter()
                .map(|source| LibrarySource {
                    file_name: source.file_name.clone(),
                    source: source.source.clone(),
                    category: source.category.clone(),
                })
                .collect()
        },
        global_constants: options.global_constants.clone(),
        dependencies: options
            .dependencies
            .iter()
            .map(|dependency| LibraryDependency {
                name: dependency.manifest.name.clone(),
                version: dependency.manifest.version.clone(),
            })
            .collect(),
    };
    archive.validate(options.name)?;
    Ok(archive)
}

fn tag_source_metadata(
    unit: &CompilationUnit,
    category: Option<&str>,
    documentation: Option<&str>,
    category_by_name: &mut HashMap<String, String>,
    documentation_by_name: &mut HashMap<String, String>,
) {
    let names = unit
        .functions
        .iter()
        .map(|item| item.name.as_str())
        .chain(unit.function_blocks.iter().map(|item| item.name.as_str()))
        .chain(unit.types.iter().map(|item| item.name.as_str()))
        .chain(unit.interfaces.iter().map(|item| item.name.as_str()));
    for name in names {
        let key = name.to_ascii_uppercase();
        if let Some(category) = category.filter(|value| !value.is_empty()) {
            category_by_name
                .entry(key.clone())
                .or_insert_with(|| category.to_owned());
        }
        if let Some(documentation) = documentation.filter(|value| !value.is_empty()) {
            documentation_by_name
                .entry(key)
                .or_insert_with(|| documentation.to_owned());
        }
    }
    for block in &unit.global_var_blocks {
        for declaration in &block.declarations {
            for name in &declaration.names {
                if let Some(category) = category.filter(|value| !value.is_empty()) {
                    category_by_name
                        .entry(name.to_ascii_uppercase())
                        .or_insert_with(|| category.to_owned());
                }
            }
        }
    }
}

fn manifest_from_ast(
    ast: &CompilationUnit,
    options: &CompileStlibOptions<'_>,
    sources: &[LibrarySourceInput],
    category_by_name: &HashMap<String, String>,
    documentation_by_name: &HashMap<String, String>,
) -> LibraryManifest {
    LibraryManifest {
        name: options.name.to_owned(),
        display_name: None,
        version: options.version.to_owned(),
        description: None,
        namespace: options.namespace.to_owned(),
        functions: ast
            .functions
            .iter()
            .map(|function| LibraryFunctionEntry {
                name: function.name.clone(),
                return_type: function.return_type.name.clone(),
                parameters: function
                    .var_blocks
                    .iter()
                    .flat_map(|block| {
                        block.declarations.iter().flat_map(move |declaration| {
                            declaration
                                .names
                                .iter()
                                .map(move |name| LibraryFunctionParameter {
                                    name: name.clone(),
                                    type_name: declaration.variable_type.name.clone(),
                                    direction: match block.block_type {
                                        VarBlockType::Output => "output",
                                        VarBlockType::InOut => "inout",
                                        _ => "input",
                                    }
                                    .to_owned(),
                                    initial_value: declaration
                                        .initial_value
                                        .as_ref()
                                        .and_then(serialize_initial_value),
                                })
                        })
                    })
                    .collect(),
                variadic: None,
                documentation: documentation_by_name
                    .get(&function.name.to_ascii_uppercase())
                    .cloned(),
                category: category_by_name
                    .get(&function.name.to_ascii_uppercase())
                    .cloned(),
            })
            .collect(),
        function_blocks: ast
            .function_blocks
            .iter()
            .map(|block| LibraryFbEntry {
                name: block.name.clone(),
                inputs: variables_for_block(block, VarBlockType::Input),
                outputs: variables_for_block(block, VarBlockType::Output),
                inouts: variables_for_block(block, VarBlockType::InOut),
                documentation: documentation_by_name
                    .get(&block.name.to_ascii_uppercase())
                    .cloned(),
                category: category_by_name
                    .get(&block.name.to_ascii_uppercase())
                    .cloned(),
            })
            .collect(),
        types: ast
            .types
            .iter()
            .map(|declaration| {
                let (kind, base_type, fields) = match &declaration.definition {
                    TypeDefinition::Struct(fields) => (
                        "struct".to_owned(),
                        None,
                        fields
                            .iter()
                            .flat_map(|field| {
                                field.names.iter().map(|name| LibraryTypeField {
                                    name: name.clone(),
                                    type_name: field.variable_type.name.clone(),
                                })
                            })
                            .collect(),
                    ),
                    TypeDefinition::Enum { base_type, .. } => (
                        "enum".to_owned(),
                        base_type.as_ref().map(|base| base.name.clone()),
                        Vec::new(),
                    ),
                    TypeDefinition::Alias(base) => {
                        ("alias".to_owned(), Some(base.name.clone()), Vec::new())
                    }
                    TypeDefinition::Array { element_type, .. } => (
                        "alias".to_owned(),
                        Some(element_type.name.clone()),
                        Vec::new(),
                    ),
                    TypeDefinition::Subrange { base_type, .. } => {
                        ("alias".to_owned(), Some(base_type.name.clone()), Vec::new())
                    }
                };
                LibraryTypeEntry {
                    name: declaration.name.clone(),
                    kind,
                    base_type,
                    fields,
                    documentation: documentation_by_name
                        .get(&declaration.name.to_ascii_uppercase())
                        .cloned(),
                    category: category_by_name
                        .get(&declaration.name.to_ascii_uppercase())
                        .cloned(),
                }
            })
            .collect(),
        globals: ast
            .global_var_blocks
            .iter()
            .flat_map(|block| {
                block.declarations.iter().flat_map(move |declaration| {
                    declaration
                        .names
                        .iter()
                        .map(move |name| LibraryGlobalEntry {
                            name: name.clone(),
                            type_name: declaration.variable_type.name.clone(),
                            constant: block.is_constant,
                            category: category_by_name.get(&name.to_ascii_uppercase()).cloned(),
                        })
                })
            })
            .collect(),
        headers: Vec::new(),
        is_builtin: options.builtin,
        source_files: sources
            .iter()
            .map(|source| source.file_name.clone())
            .collect(),
    }
}

fn variables_for_block(
    block: &crate::frontend::ast::FunctionBlockDeclaration,
    kind: VarBlockType,
) -> Vec<LibraryVarType> {
    block
        .var_blocks
        .iter()
        .filter(|block| block.block_type == kind)
        .flat_map(|block| {
            block.declarations.iter().flat_map(|declaration| {
                declaration
                    .names
                    .iter()
                    .map(|name| serialize_var_type(name, &declaration.variable_type))
            })
        })
        .collect()
}

fn serialize_var_type(name: &str, type_ref: &TypeReference) -> LibraryVarType {
    LibraryVarType {
        name: name.to_owned(),
        type_kind: type_ref.name.clone(),
        array_dimensions: type_ref
            .array_dimensions
            .iter()
            .filter_map(serialize_dimension)
            .collect(),
        element_type_name: type_ref.element_type_name.clone(),
        reference_kind: match type_ref.reference_kind {
            ReferenceKind::None => None,
            ReferenceKind::RefTo => Some("refTo".to_owned()),
            ReferenceKind::ReferenceTo => Some("referenceTo".to_owned()),
            ReferenceKind::PointerTo => Some("pointerTo".to_owned()),
        },
    }
}

fn serialize_dimension(dimension: &ArrayDimension) -> Option<LibraryArrayDimension> {
    if dimension.is_variable_length {
        return None;
    }
    Some(LibraryArrayDimension {
        start: literal_i64(dimension.start.as_ref()?)?,
        end: literal_i64(dimension.end.as_ref()?)?,
    })
}

fn literal_i64(expression: &Expression) -> Option<i64> {
    match expression {
        Expression::Literal(literal) => literal.raw_value.replace('_', "").parse().ok(),
        Expression::Unary {
            operator: UnaryOperator::Negate,
            operand,
            ..
        } => literal_i64(operand).and_then(i64::checked_neg),
        Expression::Unary {
            operator: UnaryOperator::Positive,
            operand,
            ..
        }
        | Expression::Parenthesized {
            expression: operand,
            ..
        } => literal_i64(operand),
        _ => None,
    }
}

fn serialize_initial_value(expression: &Expression) -> Option<String> {
    match expression {
        Expression::Literal(literal) => Some(literal.raw_value.clone()),
        Expression::Variable(variable) if variable.access_chain.is_empty() => {
            Some(variable.name.clone())
        }
        Expression::Unary {
            operator, operand, ..
        } => {
            let operand = serialize_initial_value(operand)?;
            Some(match operator {
                UnaryOperator::Not => format!("NOT {operand}"),
                UnaryOperator::Negate => format!("-{operand}"),
                UnaryOperator::Positive => format!("+{operand}"),
            })
        }
        Expression::Parenthesized { expression, .. } => {
            Some(format!("({})", serialize_initial_value(expression)?))
        }
        Expression::Binary {
            operator,
            left,
            right,
            ..
        } => Some(format!(
            "{} {} {}",
            serialize_initial_value(left)?,
            binary_operator(*operator),
            serialize_initial_value(right)?
        )),
        _ => None,
    }
}

fn binary_operator(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Mod => "MOD",
        BinaryOperator::Power => "**",
        BinaryOperator::And => "AND",
        BinaryOperator::Or => "OR",
        BinaryOperator::Xor => "XOR",
        BinaryOperator::Equal => "=",
        BinaryOperator::NotEqual => "<>",
        BinaryOperator::Less => "<",
        BinaryOperator::Greater => ">",
        BinaryOperator::LessEqual => "<=",
        BinaryOperator::GreaterEqual => ">=",
    }
}

fn symbol_ownership(
    items: &[LibraryCodegenItem],
    dependencies: &[StlibArchive],
    own_library: &str,
) -> HashMap<String, (String, String)> {
    let mut ownership = HashMap::new();
    for item in items {
        ownership.insert(
            item.name.to_ascii_uppercase(),
            (own_library.to_owned(), item.name.clone()),
        );
    }
    for dependency in dependencies {
        for name in dependency_symbols(dependency) {
            ownership
                .entry(name.to_ascii_uppercase())
                .or_insert((dependency.manifest.name.clone(), name));
        }
    }
    ownership
}

fn dependency_symbols(archive: &StlibArchive) -> Vec<String> {
    let mut names = archive
        .manifest
        .functions
        .iter()
        .map(|item| item.name.clone())
        .chain(
            archive
                .manifest
                .function_blocks
                .iter()
                .map(|item| item.name.clone()),
        )
        .chain(archive.manifest.types.iter().map(|item| item.name.clone()))
        .chain(
            archive
                .manifest
                .globals
                .iter()
                .map(|item| item.name.clone()),
        )
        .chain(archive.chunks.iter().map(|item| item.name.clone()))
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn infer_dependencies(
    item: &LibraryCodegenItem,
    ownership: &HashMap<String, (String, String)>,
    own_library: &str,
) -> Vec<LibraryChunkDep> {
    let mut referenced = HashSet::new();
    let emitted = format!("{}\n{}", item.header, item.cpp).to_ascii_uppercase();
    for (symbol, (library, canonical_name)) in ownership {
        if symbol == &item.name.to_ascii_uppercase() {
            continue;
        }
        if source_has_identifier(&emitted, symbol) {
            referenced.insert((library.clone(), canonical_name.clone()));
        }
    }
    let mut deps = referenced
        .into_iter()
        .map(|(library, name)| LibraryChunkDep {
            library: if library == own_library {
                own_library.to_owned()
            } else {
                library
            },
            name,
        })
        .collect::<Vec<_>>();
    deps.sort_by(|left, right| {
        left.library
            .cmp(&right.library)
            .then_with(|| left.name.cmp(&right.name))
    });
    deps
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
    use super::{compile_stlib, CompileStlibOptions, LibrarySourceInput};
    use crate::backend::generate_cpp_with_libraries;
    use crate::frontend::parse;
    use std::collections::BTreeMap;

    #[test]
    fn source_library_round_trips_into_reachable_consumer_chunks() {
        let sources = [LibrarySourceInput {
            file_name: "demo.st".to_owned(),
            category: Some("Math".to_owned()),
            documentation: Some("Demo docs".to_owned()),
            source: r#"
TYPE PAIR : STRUCT
    X : INT;
END_STRUCT
END_TYPE

FUNCTION ADDONE : INT
VAR_INPUT
    A : INT;
END_VAR
ADDONE := A + 1;
END_FUNCTION

FUNCTION ADDTWO : INT
VAR_INPUT
    A : INT;
END_VAR
ADDTWO := ADDONE(ADDONE(A));
END_FUNCTION

FUNCTION UNUSED : INT
VAR_INPUT
    A : INT;
END_VAR
UNUSED := A;
END_FUNCTION
"#
            .to_owned(),
        }];
        let archive = compile_stlib(
            &sources,
            &CompileStlibOptions {
                name: "demo-lib",
                version: "1.0.0",
                namespace: "demo_lib",
                no_source: false,
                builtin: false,
                dependencies: &[],
                global_constants: BTreeMap::new(),
            },
        )
        .unwrap();
        assert_eq!(archive.manifest.functions.len(), 3);
        assert_eq!(archive.manifest.types.len(), 1);
        assert_eq!(archive.sources.len(), 1);
        assert_eq!(
            archive.manifest.functions[0].category.as_deref(),
            Some("Math")
        );
        let addtwo = archive
            .chunks
            .iter()
            .find(|chunk| chunk.name == "ADDTWO")
            .unwrap();
        assert!(addtwo.deps.iter().any(|dep| dep.name == "ADDONE"));

        let consumer = r#"
PROGRAM MAIN
VAR
    R : INT;
END_VAR
R := ADDTWO(40);
END_PROGRAM
CONFIGURATION Config0
RESOURCE Res0 ON PLC
TASK task0(INTERVAL := T#20ms, PRIORITY := 1);
PROGRAM instance0 WITH task0 : MAIN;
END_RESOURCE
END_CONFIGURATION
"#;
        let ast = parse(consumer, "consumer.st").unwrap();
        let generated = generate_cpp_with_libraries(&ast, &[archive], consumer).unwrap();
        assert!(generated
            .cpp_files
            .iter()
            .any(|file| file.name == "lib_demo_lib_addtwo.cpp"));
        assert!(generated
            .cpp_files
            .iter()
            .any(|file| file.name == "lib_demo_lib_addone.cpp"));
        assert!(!generated
            .cpp_files
            .iter()
            .any(|file| file.name == "lib_demo_lib_unused.cpp"));
    }

    #[test]
    fn registered_library_global_constant_resolves_array_bounds() {
        let source = [LibrarySourceInput {
            file_name: "buffer.st".to_owned(),
            category: None,
            documentation: None,
            source: "TYPE BUFFER : ARRAY [0..STRING_LENGTH] OF BYTE; END_TYPE\n".to_owned(),
        }];
        let mut constants = BTreeMap::new();
        constants.insert("STRING_LENGTH".to_owned(), 63.0);
        let archive = compile_stlib(
            &source,
            &CompileStlibOptions {
                name: "constants-lib",
                version: "1.0.0",
                namespace: "constants_lib",
                no_source: false,
                builtin: false,
                dependencies: &[],
                global_constants: constants,
            },
        )
        .unwrap();
        assert!(
            archive.chunks[0].header.contains(", 0, 63>"),
            "{}",
            archive.chunks[0].header
        );

        let consumer = r#"
PROGRAM MAIN
VAR
    DATA : ARRAY [0..STRING_LENGTH] OF BYTE;
END_VAR
END_PROGRAM
CONFIGURATION Config0
RESOURCE Res0 ON PLC
TASK task0(INTERVAL := T#20ms, PRIORITY := 1);
PROGRAM instance0 WITH task0 : MAIN;
END_RESOURCE
END_CONFIGURATION
"#;
        let ast = parse(consumer, "constant-consumer.st").unwrap();
        let generated = generate_cpp_with_libraries(&ast, &[archive], consumer).unwrap();
        assert!(
            generated.header_code.contains(", 0, 63>"),
            "{}",
            generated.header_code
        );
    }
}
