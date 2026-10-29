use crate::frontend::ast::{ReferenceKind, TypeReference};

use super::names::iec_ident;
use super::type_registry::TypeRegistry;

pub fn cpp_type_with_registry(
    type_ref: &TypeReference,
    registry: &TypeRegistry,
) -> Result<String, String> {
    if !type_ref.array_dimensions.is_empty() {
        let Some(element) = type_ref.element_type_name.as_deref() else {
            return Err("inline array type is missing element type".to_owned());
        };
        let element = struct_field_type_name(element, None, registry);
        let has_variable_length = type_ref
            .array_dimensions
            .iter()
            .any(|dimension| dimension.is_variable_length);
        let base = if has_variable_length {
            if !type_ref
                .array_dimensions
                .iter()
                .all(|dimension| dimension.is_variable_length)
            {
                return Err(
                    "mixed fixed and variable-length array dimensions are unsupported".to_owned(),
                );
            }
            match type_ref.array_dimensions.len() {
                1 => format!("ArrayView1D<{element}>"),
                2 => format!("ArrayView2D<{element}>"),
                dimensions => {
                    return Err(format!(
                        "variable-length arrays with {dimensions} dimensions are unsupported"
                    ))
                }
            }
        } else {
            registry.format_array_type(&element, &type_ref.array_dimensions)?
        };
        return apply_reference_kind(base, type_ref.reference_kind, element.as_str());
    }

    let upper = iec_ident(&type_ref.name);
    let base = if registry.is_user_type(&upper) {
        if registry.is_enum(&upper) {
            format!("IEC_{upper}")
        } else {
            upper.clone()
        }
    } else {
        elementary_var_type(&upper, type_ref.max_length.as_deref()).unwrap_or_else(|| upper.clone())
    };
    apply_reference_kind(base, type_ref.reference_kind, &upper)
}

pub fn raw_type_with_registry(
    type_ref: &TypeReference,
    registry: &TypeRegistry,
) -> Result<String, String> {
    if !type_ref.array_dimensions.is_empty() {
        let Some(element) = type_ref.element_type_name.as_deref() else {
            return Err("inline array type is missing element type".to_owned());
        };
        let element = raw_named_type(element, registry);
        return registry.format_array_type(&element, &type_ref.array_dimensions);
    }
    let upper = iec_ident(&type_ref.name);
    let mut base = raw_named_type(&upper, registry);
    if type_ref.reference_kind == ReferenceKind::PointerTo {
        base.push('*');
    }
    Ok(base)
}

pub fn struct_field_cpp_type(
    type_ref: &TypeReference,
    registry: &TypeRegistry,
) -> Result<String, String> {
    let mut base = if !type_ref.array_dimensions.is_empty() {
        let Some(element) = type_ref.element_type_name.as_deref() else {
            return Err("inline array type is missing element type".to_owned());
        };
        let element = struct_field_type_name(element, None, registry);
        registry.format_array_type(&element, &type_ref.array_dimensions)?
    } else {
        struct_field_type_name(&type_ref.name, type_ref.max_length.as_deref(), registry)
    };
    if type_ref.reference_kind == ReferenceKind::PointerTo {
        base.push('*');
    }
    Ok(base)
}

pub fn raw_cpp_type(type_name: &str) -> String {
    elementary_raw_type(&iec_ident(type_name)).unwrap_or_else(|| iec_ident(type_name))
}

pub fn default_value(type_ref: &TypeReference) -> String {
    match iec_ident(&type_ref.name).as_str() {
        "BOOL" => "false".to_owned(),
        "REAL" | "LREAL" => "0.0".to_owned(),
        "STRING" | "WSTRING" => "\"\"".to_owned(),
        _ => "0".to_owned(),
    }
}

pub fn alias_iec_wrapper(name: &str, base: &TypeReference, registry: &TypeRegistry) -> String {
    let name = iec_ident(name);
    let base_name = iec_ident(&base.name);
    if is_elementary(&base_name) {
        let wrapper = elementary_var_type(&base_name, base.max_length.as_deref())
            .unwrap_or_else(|| format!("IEC_{base_name}"));
        format!("using IEC_{name} = {wrapper};")
    } else if registry.is_enum(&base_name) {
        format!("using IEC_{name} = IEC_{base_name};")
    } else {
        format!("using IEC_{name} = IECVar<{name}>;")
    }
}

pub fn subrange_iec_wrapper(name: &str, base: &TypeReference, registry: &TypeRegistry) -> String {
    alias_iec_wrapper(name, base, registry)
}

pub fn is_elementary(name: &str) -> bool {
    elementary_raw_type(&iec_ident(name)).is_some()
}

fn apply_reference_kind(
    base: String,
    reference_kind: ReferenceKind,
    raw_name: &str,
) -> Result<String, String> {
    Ok(match reference_kind {
        ReferenceKind::None => base,
        ReferenceKind::PointerTo => format!("IEC_Ptr<{}>", raw_cpp_type(raw_name)),
        ReferenceKind::RefTo => format!("IEC_REF_TO<{}>", raw_cpp_type(raw_name)),
        ReferenceKind::ReferenceTo => format!("IEC_REFERENCE_TO<{}>", raw_cpp_type(raw_name)),
    })
}

fn raw_named_type(type_name: &str, registry: &TypeRegistry) -> String {
    let upper = iec_ident(type_name);
    if registry.is_user_type(&upper) {
        upper
    } else {
        elementary_raw_type(&upper).unwrap_or(upper)
    }
}

fn struct_field_type_name(
    type_name: &str,
    max_length: Option<&str>,
    registry: &TypeRegistry,
) -> String {
    let upper = iec_ident(type_name);
    if registry.is_enum(&upper) {
        return format!("IEC_{upper}");
    }
    if registry.is_user_type(&upper) {
        return upper;
    }
    elementary_var_type(&upper, max_length).unwrap_or(upper)
}

fn elementary_raw_type(name: &str) -> Option<String> {
    Some(match name {
        "BOOL" => "BOOL_t".to_owned(),
        "BYTE" => "BYTE_t".to_owned(),
        "WORD" => "WORD_t".to_owned(),
        "DWORD" => "DWORD_t".to_owned(),
        "LWORD" => "LWORD_t".to_owned(),
        "__XWORD" => "XWORD_t".to_owned(),
        "SINT" => "SINT_t".to_owned(),
        "INT" => "INT_t".to_owned(),
        "DINT" => "DINT_t".to_owned(),
        "LINT" => "LINT_t".to_owned(),
        "USINT" => "USINT_t".to_owned(),
        "UINT" => "UINT_t".to_owned(),
        "UDINT" => "UDINT_t".to_owned(),
        "ULINT" => "ULINT_t".to_owned(),
        "REAL" => "REAL_t".to_owned(),
        "LREAL" => "LREAL_t".to_owned(),
        "TIME" => "TIME_t".to_owned(),
        "DATE" => "DATE_t".to_owned(),
        "TOD" | "TIME_OF_DAY" => "TOD_t".to_owned(),
        "DT" | "DATE_AND_TIME" => "DT_t".to_owned(),
        "LTIME" => "LTIME_t".to_owned(),
        "LDATE" => "LDATE_t".to_owned(),
        "LTOD" => "LTOD_t".to_owned(),
        "LDT" => "LDT_t".to_owned(),
        "CHAR" => "CHAR_t".to_owned(),
        "WCHAR" => "WCHAR_t".to_owned(),
        "STRING" => "IECString<254>".to_owned(),
        "WSTRING" => "IECWString<254>".to_owned(),
        _ => return None,
    })
}

fn elementary_var_type(name: &str, max_length: Option<&str>) -> Option<String> {
    Some(match name {
        "BOOL" => "IEC_BOOL".to_owned(),
        "BYTE" => "IEC_BYTE".to_owned(),
        "WORD" => "IEC_WORD".to_owned(),
        "DWORD" => "IEC_DWORD".to_owned(),
        "LWORD" => "IEC_LWORD".to_owned(),
        "__XWORD" => "IEC_XWORD".to_owned(),
        "SINT" => "IEC_SINT".to_owned(),
        "INT" => "IEC_INT".to_owned(),
        "DINT" => "IEC_DINT".to_owned(),
        "LINT" => "IEC_LINT".to_owned(),
        "USINT" => "IEC_USINT".to_owned(),
        "UINT" => "IEC_UINT".to_owned(),
        "UDINT" => "IEC_UDINT".to_owned(),
        "ULINT" => "IEC_ULINT".to_owned(),
        "REAL" => "IEC_REAL".to_owned(),
        "LREAL" => "IEC_LREAL".to_owned(),
        "TIME" => "IEC_TIME".to_owned(),
        "DATE" => "IEC_DATE".to_owned(),
        "TOD" | "TIME_OF_DAY" => "IEC_TOD".to_owned(),
        "DT" | "DATE_AND_TIME" => "IEC_DT".to_owned(),
        "LTIME" => "IEC_LTIME".to_owned(),
        "LDATE" => "IEC_LDATE".to_owned(),
        "LTOD" => "IEC_LTOD".to_owned(),
        "LDT" => "IEC_LDT".to_owned(),
        "CHAR" => "IEC_CHAR".to_owned(),
        "WCHAR" => "IEC_WCHAR".to_owned(),
        "STRING" => format!("IECStringVar<{}>", max_length.unwrap_or("254")),
        "WSTRING" => format!("IECWStringVar<{}>", max_length.unwrap_or("254")),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::frontend::parse;

    use super::{cpp_type_with_registry, struct_field_cpp_type};
    use crate::backend::type_registry::TypeRegistry;

    #[test]
    fn maps_user_types_like_strucpp() {
        let unit = parse(
            "TYPE MyInt : INT; Mode : (A,B); Pair : STRUCT x : DINT; END_STRUCT; Values : ARRAY[0..3] OF DINT; END_TYPE PROGRAM main VAR a : MyInt; m : Mode; p : Pair; v : Values; END_VAR END_PROGRAM",
            "types.st",
        )
        .unwrap();
        let registry = TypeRegistry::new(&unit.types);
        let declarations = &unit.programs[0].var_blocks[0].declarations;
        assert_eq!(
            cpp_type_with_registry(&declarations[0].variable_type, &registry).unwrap(),
            "MYINT"
        );
        assert_eq!(
            cpp_type_with_registry(&declarations[1].variable_type, &registry).unwrap(),
            "IEC_MODE"
        );
        assert_eq!(
            cpp_type_with_registry(&declarations[2].variable_type, &registry).unwrap(),
            "PAIR"
        );
        assert_eq!(
            cpp_type_with_registry(&declarations[3].variable_type, &registry).unwrap(),
            "VALUES"
        );
        let field = match &unit.types[2].definition {
            crate::frontend::ast::TypeDefinition::Struct(fields) => &fields[0].variable_type,
            _ => panic!("struct"),
        };
        assert_eq!(struct_field_cpp_type(field, &registry).unwrap(), "IEC_DINT");
    }
}
