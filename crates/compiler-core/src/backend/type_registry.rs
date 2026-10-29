use std::collections::{BTreeMap, HashMap, HashSet};

use crate::frontend::ast::{
    ArrayDimension, Expression, LiteralType, TypeDeclaration, TypeDefinition, TypeReference,
    UnaryOperator, VarBlock,
};
use crate::library::StlibArchive;

use super::names::iec_ident;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserTypeKind {
    Alias,
    Enum,
    Struct,
    Array,
    Subrange,
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub cpp_name: String,
    pub type_name: String,
    pub is_array: bool,
    pub array_element_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TypeInfo {
    pub kind: UserTypeKind,
    pub dependencies: Vec<String>,
    pub fields: HashMap<String, FieldInfo>,
    pub array_element_type: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TypeRegistry {
    types: HashMap<String, TypeInfo>,
    declaration_index: HashMap<String, usize>,
    enum_member_owner: HashMap<String, String>,
    constants: HashMap<String, i64>,
}

impl TypeRegistry {
    pub fn new(declarations: &[TypeDeclaration]) -> Self {
        let declaration_index = declarations
            .iter()
            .enumerate()
            .map(|(index, declaration)| (iec_ident(&declaration.name), index))
            .collect::<HashMap<_, _>>();
        let user_names = declaration_index.keys().cloned().collect::<HashSet<_>>();
        let mut registry = Self {
            types: HashMap::new(),
            declaration_index,
            enum_member_owner: HashMap::new(),
            constants: HashMap::new(),
        };

        for declaration in declarations {
            let name = iec_ident(&declaration.name);
            let (kind, dependencies, fields, array_element_type, enum_members) =
                match &declaration.definition {
                    TypeDefinition::Alias(type_ref) => (
                        UserTypeKind::Alias,
                        dependency_for_type_ref(type_ref, &user_names),
                        HashMap::new(),
                        None,
                        Vec::new(),
                    ),
                    TypeDefinition::Enum {
                        base_type, members, ..
                    } => (
                        UserTypeKind::Enum,
                        base_type
                            .as_ref()
                            .map(|type_ref| dependency_for_type_ref(type_ref, &user_names))
                            .unwrap_or_default(),
                        HashMap::new(),
                        None,
                        members
                            .iter()
                            .map(|member| iec_ident(&member.name))
                            .collect(),
                    ),
                    TypeDefinition::Struct(fields) => {
                        let mut dependencies = Vec::new();
                        let mut mapped = HashMap::new();
                        for field in fields {
                            let type_name = iec_ident(&field.variable_type.name);
                            if user_names.contains(&type_name)
                                && !dependencies
                                    .iter()
                                    .any(|dependency| dependency == &type_name)
                            {
                                dependencies.push(type_name.clone());
                            }
                            for field_name in &field.names {
                                let field_key = iec_ident(field_name);
                                let cpp_name =
                                    if user_names.contains(&type_name) && field_key == type_name {
                                        format!("{field_key}_")
                                    } else {
                                        field_key.clone()
                                    };
                                mapped.insert(
                                    field_key,
                                    FieldInfo {
                                        cpp_name,
                                        type_name: type_name.clone(),
                                        is_array: !field.variable_type.array_dimensions.is_empty(),
                                        array_element_type: field
                                            .variable_type
                                            .element_type_name
                                            .as_deref()
                                            .map(iec_ident),
                                    },
                                );
                            }
                        }
                        (UserTypeKind::Struct, dependencies, mapped, None, Vec::new())
                    }
                    TypeDefinition::Array { element_type, .. } => {
                        let element = iec_ident(&element_type.name);
                        (
                            UserTypeKind::Array,
                            if user_names.contains(&element) {
                                vec![element.clone()]
                            } else {
                                Vec::new()
                            },
                            HashMap::new(),
                            Some(element),
                            Vec::new(),
                        )
                    }
                    TypeDefinition::Subrange { base_type, .. } => (
                        UserTypeKind::Subrange,
                        dependency_for_type_ref(base_type, &user_names),
                        HashMap::new(),
                        None,
                        Vec::new(),
                    ),
                };
            for member in &enum_members {
                registry
                    .enum_member_owner
                    .entry(member.clone())
                    .or_insert_with(|| name.clone());
            }
            registry.types.insert(
                name,
                TypeInfo {
                    kind,
                    dependencies,
                    fields,
                    array_element_type,
                },
            );
        }
        registry
    }

    pub fn register_libraries(&mut self, archives: &[StlibArchive]) {
        for archive in archives {
            self.register_constants(&archive.global_constants);
        }
        let mut known = self.types.keys().cloned().collect::<HashSet<_>>();
        for archive in archives {
            for library_type in &archive.manifest.types {
                known.insert(iec_ident(&library_type.name));
            }
        }
        for archive in archives {
            for library_type in &archive.manifest.types {
                let name = iec_ident(&library_type.name);
                if self.types.contains_key(&name) {
                    continue;
                }
                let mut fields = HashMap::new();
                let dependencies = library_type
                    .base_type
                    .as_deref()
                    .map(iec_ident)
                    .filter(|base| known.contains(base))
                    .into_iter()
                    .collect::<Vec<_>>();
                for field in &library_type.fields {
                    let field_name = iec_ident(&field.name);
                    let type_name = iec_ident(&field.type_name);
                    fields.insert(
                        field_name.clone(),
                        FieldInfo {
                            cpp_name: if known.contains(&type_name) && field_name == type_name {
                                format!("{field_name}_")
                            } else {
                                field_name
                            },
                            type_name,
                            is_array: false,
                            array_element_type: None,
                        },
                    );
                }
                let kind = match library_type.kind.as_str() {
                    "struct" => UserTypeKind::Struct,
                    "enum" => UserTypeKind::Enum,
                    _ => UserTypeKind::Alias,
                };
                self.types.insert(
                    name,
                    TypeInfo {
                        kind,
                        dependencies,
                        fields,
                        array_element_type: None,
                    },
                );
            }
        }
    }

    pub fn is_user_type(&self, name: &str) -> bool {
        self.types.contains_key(&iec_ident(name))
    }

    pub fn kind(&self, name: &str) -> Option<UserTypeKind> {
        self.types.get(&iec_ident(name)).map(|info| info.kind)
    }

    pub fn is_enum(&self, name: &str) -> bool {
        self.kind(name) == Some(UserTypeKind::Enum)
    }

    pub fn is_array(&self, name: &str) -> bool {
        self.kind(name) == Some(UserTypeKind::Array)
    }

    pub fn is_composite(&self, name: &str) -> bool {
        let name = iec_ident(name);
        let mut visiting = HashSet::new();
        self.is_composite_inner(&name, &mut visiting)
    }

    fn is_composite_inner(&self, name: &str, visiting: &mut HashSet<String>) -> bool {
        if !visiting.insert(name.to_owned()) {
            return false;
        }
        let result = match self.types.get(name) {
            Some(info) if matches!(info.kind, UserTypeKind::Struct | UserTypeKind::Array) => true,
            Some(info) if info.kind == UserTypeKind::Alias => info
                .dependencies
                .iter()
                .any(|dependency| self.is_composite_inner(dependency, visiting)),
            _ => false,
        };
        visiting.remove(name);
        result
    }

    pub fn enum_member_owner(&self, member: &str) -> Option<&str> {
        self.enum_member_owner
            .get(&iec_ident(member))
            .map(String::as_str)
    }

    pub fn field(&self, type_name: &str, field: &str) -> Option<&FieldInfo> {
        self.types
            .get(&iec_ident(type_name))?
            .fields
            .get(&iec_ident(field))
    }

    pub fn array_element_type(&self, type_name: &str) -> Option<&str> {
        self.types
            .get(&iec_ident(type_name))?
            .array_element_type
            .as_deref()
    }

    pub fn member_cpp_name(&self, variable_name: &str, type_name: &str) -> String {
        let variable = iec_ident(variable_name);
        let type_name = iec_ident(type_name);
        if self.is_user_type(&type_name) && variable == type_name {
            format!("{variable}_")
        } else {
            variable
        }
    }

    pub fn ordered<'a>(&self, declarations: &'a [TypeDeclaration]) -> Vec<&'a TypeDeclaration> {
        let mut visiting = HashSet::new();
        let mut emitted = HashSet::new();
        let mut result = Vec::new();

        fn visit<'a>(
            name: &str,
            registry: &TypeRegistry,
            declarations: &'a [TypeDeclaration],
            visiting: &mut HashSet<String>,
            emitted: &mut HashSet<String>,
            result: &mut Vec<&'a TypeDeclaration>,
        ) {
            if emitted.contains(name) {
                return;
            }
            if !visiting.insert(name.to_owned()) {
                return;
            }
            if let Some(info) = registry.types.get(name) {
                for dependency in &info.dependencies {
                    visit(
                        dependency,
                        registry,
                        declarations,
                        visiting,
                        emitted,
                        result,
                    );
                }
            }
            visiting.remove(name);
            if emitted.insert(name.to_owned()) {
                if let Some(index) = registry.declaration_index.get(name).copied() {
                    result.push(&declarations[index]);
                }
            }
        }

        for declaration in declarations {
            visit(
                &iec_ident(&declaration.name),
                self,
                declarations,
                &mut visiting,
                &mut emitted,
                &mut result,
            );
        }
        result
    }

    pub fn register_constants(&mut self, constants: &BTreeMap<String, f64>) {
        for (name, value) in constants {
            if value.is_finite()
                && value.fract() == 0.0
                && *value >= i64::MIN as f64
                && *value <= i64::MAX as f64
            {
                self.constants.insert(iec_ident(name), *value as i64);
            }
        }
    }

    pub fn register_block_constants(&mut self, blocks: &[VarBlock]) {
        let mut pending = blocks
            .iter()
            .filter(|block| block.is_constant)
            .flat_map(|block| block.declarations.iter())
            .filter_map(|declaration| {
                declaration
                    .initial_value
                    .as_ref()
                    .map(|value| (declaration.names.as_slice(), value))
            })
            .collect::<Vec<_>>();
        loop {
            let before = pending.len();
            pending.retain(|(names, expression)| {
                let Some(value) = self.constant_i64(Some(expression)) else {
                    return true;
                };
                for name in *names {
                    self.constants.insert(iec_ident(name), value);
                }
                false
            });
            if pending.is_empty() || pending.len() == before {
                break;
            }
        }
    }

    pub fn constant_i64(&self, expression: Option<&Expression>) -> Option<i64> {
        use crate::frontend::ast::BinaryOperator;
        match expression? {
            Expression::Variable(variable) if variable.access_chain.is_empty() => {
                self.constants.get(&iec_ident(&variable.name)).copied()
            }
            Expression::Literal(literal) if literal.literal_type == LiteralType::Int => {
                parse_integer(&literal.raw_value)
            }
            Expression::Unary {
                operator: UnaryOperator::Negate,
                operand,
                ..
            } => self.constant_i64(Some(operand)).and_then(i64::checked_neg),
            Expression::Unary {
                operator: UnaryOperator::Positive,
                operand,
                ..
            } => self.constant_i64(Some(operand)),
            Expression::Parenthesized { expression, .. } => self.constant_i64(Some(expression)),
            Expression::Binary {
                operator,
                left,
                right,
                ..
            } => {
                let left = self.constant_i64(Some(left))?;
                let right = self.constant_i64(Some(right))?;
                Some(match operator {
                    BinaryOperator::Add => left.checked_add(right)?,
                    BinaryOperator::Subtract => left.checked_sub(right)?,
                    BinaryOperator::Multiply => left.checked_mul(right)?,
                    BinaryOperator::Divide => left.checked_div(right)?,
                    BinaryOperator::Mod => left.checked_rem(right)?,
                    BinaryOperator::Power => left.checked_pow(u32::try_from(right).ok()?)?,
                    _ => return None,
                })
            }
            _ => None,
        }
    }

    pub fn format_array_type(
        &self,
        element_cpp: &str,
        dimensions: &[ArrayDimension],
    ) -> Result<String, String> {
        let bounds = dimensions
            .iter()
            .map(|dimension| {
                if dimension.is_variable_length {
                    return Err("variable-length array type requires ArrayView codegen".to_owned());
                }
                let start = self.constant_i64(dimension.start.as_ref()).ok_or_else(|| {
                    "array lower bound must be a constant integer expression".to_owned()
                })?;
                let end = self.constant_i64(dimension.end.as_ref()).ok_or_else(|| {
                    "array upper bound must be a constant integer expression".to_owned()
                })?;
                Ok((start, end))
            })
            .collect::<Result<Vec<_>, String>>()?;
        match bounds.as_slice() {
            [(start, end)] => Ok(format!("Array1D<{element_cpp}, {start}, {end}>")),
            [(s1, e1), (s2, e2)] => Ok(format!("Array2D<{element_cpp}, {s1}, {e1}, {s2}, {e2}>")),
            [(s1, e1), (s2, e2), (s3, e3)] => Ok(format!(
                "Array3D<{element_cpp}, {s1}, {e1}, {s2}, {e2}, {s3}, {e3}>"
            )),
            [] => Err("array type has no dimensions".to_owned()),
            _ => {
                let mut result = element_cpp.to_owned();
                for (start, end) in bounds.iter().rev() {
                    result = format!("Array1D<{result}, {start}, {end}>");
                }
                Ok(result)
            }
        }
    }
}

fn dependency_for_type_ref(type_ref: &TypeReference, user_names: &HashSet<String>) -> Vec<String> {
    let name = iec_ident(&type_ref.name);
    if user_names.contains(&name) {
        vec![name]
    } else {
        Vec::new()
    }
}

pub fn constant_i64(expression: Option<&Expression>) -> Option<i64> {
    use crate::frontend::ast::BinaryOperator;
    match expression? {
        Expression::Literal(literal) if literal.literal_type == LiteralType::Int => {
            parse_integer(&literal.raw_value)
        }
        Expression::Unary {
            operator: UnaryOperator::Negate,
            operand,
            ..
        } => constant_i64(Some(operand)).map(|value| -value),
        Expression::Unary {
            operator: UnaryOperator::Positive,
            operand,
            ..
        } => constant_i64(Some(operand)),
        Expression::Binary {
            operator,
            left,
            right,
            ..
        } => {
            let left = constant_i64(Some(left))?;
            let right = constant_i64(Some(right))?;
            Some(match operator {
                BinaryOperator::Add => left.checked_add(right)?,
                BinaryOperator::Subtract => left.checked_sub(right)?,
                BinaryOperator::Multiply => left.checked_mul(right)?,
                BinaryOperator::Divide => left.checked_div(right)?,
                BinaryOperator::Mod => left.checked_rem(right)?,
                BinaryOperator::Power => left.checked_pow(u32::try_from(right).ok()?)?,
                _ => return None,
            })
        }
        _ => None,
    }
}

fn parse_integer(raw: &str) -> Option<i64> {
    let raw = raw.replace('_', "");
    if let Some(value) = raw.strip_prefix("16#") {
        i64::from_str_radix(value, 16).ok()
    } else if let Some(value) = raw.strip_prefix("8#") {
        i64::from_str_radix(value, 8).ok()
    } else if let Some(value) = raw.strip_prefix("2#") {
        i64::from_str_radix(value, 2).ok()
    } else {
        raw.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use crate::frontend::parse;

    use super::{TypeRegistry, UserTypeKind};

    #[test]
    fn orders_dependencies_and_records_type_shapes() {
        let unit = parse(
            "TYPE Outer : Inner; Inner : INT; Mode : (A,B); Pair : STRUCT x : DINT; END_STRUCT; Values : ARRAY[0..3] OF Pair; END_TYPE",
            "types.st",
        )
        .unwrap();
        let registry = TypeRegistry::new(&unit.types);
        let order = registry
            .ordered(&unit.types)
            .iter()
            .map(|declaration| declaration.name.to_ascii_uppercase())
            .collect::<Vec<_>>();
        assert!(
            order.iter().position(|name| name == "INNER").unwrap()
                < order.iter().position(|name| name == "OUTER").unwrap()
        );
        assert_eq!(registry.kind("Mode"), Some(UserTypeKind::Enum));
        assert_eq!(registry.kind("Values"), Some(UserTypeKind::Array));
        assert_eq!(registry.array_element_type("Values"), Some("PAIR"));
        assert_eq!(registry.field("Pair", "x").unwrap().type_name, "DINT");
    }
}
