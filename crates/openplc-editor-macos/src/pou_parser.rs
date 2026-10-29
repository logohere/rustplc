use std::sync::OnceLock;

use regex::{Regex, RegexBuilder};
use serde_json::Value;

use crate::plc_model::{ParsedPou, PouBody, PouInterface, PouLanguage, PouType};
use crate::variable_parser::parse_iec_string_to_variables;

#[derive(Debug, Clone, PartialEq)]
pub struct FallbackPou {
    pub pou: ParsedPou,
    pub variables_text: String,
}

pub fn parse_pou_from_string(
    content: &str,
    language: PouLanguage,
    pou_type: PouType,
    function_blocks: &[String],
) -> Result<ParsedPou, String> {
    if language.is_textual() {
        parse_textual_pou(content, language, pou_type, function_blocks)
    } else if language.is_graphical() {
        parse_graphical_pou(content, language, pou_type, function_blocks)
    } else {
        parse_hybrid_pou(content, language, pou_type, function_blocks)
    }
}

pub fn find_last_end_var_index(content: &str, start_index: usize) -> Option<usize> {
    let regex = end_var_regex();
    let mut last = None;
    let mut search_index = start_index;
    while search_index <= content.len() {
        let Some(found) = regex.find(&content[search_index..]) else {
            break;
        };
        let end = search_index + found.end();
        last = Some(end);
        search_index = end;
    }
    last
}

pub fn create_fallback_pou(
    content: &str,
    language: PouLanguage,
    pou_type: PouType,
    pou_name: &str,
) -> FallbackPou {
    let (documentation, remaining) = extract_documentation(content);
    let declaration = declaration_regex(pou_type);
    let mut body_start_index = declaration
        .find(remaining)
        .map(|matched| matched.end())
        .unwrap_or(0);

    let mut variables_text = "VAR\nEND_VAR".to_owned();
    if let Some(var_start) = first_var_start(remaining) {
        if let Some(last_end_var) = find_last_end_var_index(remaining, var_start) {
            variables_text = remaining[var_start..last_end_var].to_owned();
            body_start_index = last_end_var;
        }
    }

    let tail = &remaining[body_start_index..];
    let body_text = keyword_regex(pou_type.end_keyword())
        .find(tail)
        .map(|end| tail[..end.start()].trim())
        .unwrap_or_else(|| tail.trim());
    let body = if language.is_graphical() {
        let value = serde_json::from_str(body_text).unwrap_or_else(|_| {
            serde_json::json!({
                "nodes": [],
                "edges": [],
                "viewport": { "x": 0, "y": 0, "zoom": 1 }
            })
        });
        PouBody { language, value }
    } else {
        PouBody::text(language, body_text)
    };

    FallbackPou {
        pou: ParsedPou {
            name: pou_name.to_owned(),
            pou_type,
            interface: PouInterface {
                return_type: (pou_type == PouType::Function).then(|| "BOOL".to_owned()),
                variables: Vec::new(),
            },
            body,
            documentation,
        },
        variables_text,
    }
}

pub fn parse_textual_pou(
    content: &str,
    language: PouLanguage,
    pou_type: PouType,
    function_blocks: &[String],
) -> Result<ParsedPou, String> {
    if !matches!(language, PouLanguage::St | PouLanguage::Il) {
        return Err(format!(
            "Failed to parse textual POU: Parse error: unsupported textual language: {language:?}"
        ));
    }
    parse_common(
        content,
        language,
        pou_type,
        function_blocks,
        BodyMode::Textual,
    )
    .map_err(|error| format!("Failed to parse textual POU: {error}"))
}

pub fn parse_hybrid_pou(
    content: &str,
    language: PouLanguage,
    pou_type: PouType,
    function_blocks: &[String],
) -> Result<ParsedPou, String> {
    if !matches!(language, PouLanguage::Python | PouLanguage::Cpp) {
        return Err(format!(
            "Failed to parse hybrid POU: Parse error: unsupported hybrid language: {language:?}"
        ));
    }
    parse_common(
        content,
        language,
        pou_type,
        function_blocks,
        BodyMode::Hybrid,
    )
    .map_err(|error| format!("Failed to parse hybrid POU: {error}"))
}

pub fn parse_graphical_pou(
    content: &str,
    language: PouLanguage,
    pou_type: PouType,
    function_blocks: &[String],
) -> Result<ParsedPou, String> {
    if !matches!(language, PouLanguage::Ld | PouLanguage::Fbd) {
        return Err(format!(
            "Failed to parse graphical POU: Parse error: unsupported graphical language: {language:?}"
        ));
    }
    parse_common(
        content,
        language,
        pou_type,
        function_blocks,
        BodyMode::Graphical,
    )
    .map_err(|error| format!("Failed to parse graphical POU: {error}"))
}

#[derive(Clone, Copy)]
enum BodyMode {
    Textual,
    Hybrid,
    Graphical,
}

fn parse_common(
    content: &str,
    language: PouLanguage,
    pou_type: PouType,
    function_blocks: &[String],
    mode: BodyMode,
) -> Result<ParsedPou, String> {
    let (documentation, remaining) = extract_documentation(content);
    let declaration = declaration_regex(pou_type);
    let captures = declaration.captures(remaining).ok_or_else(|| {
        parse_error(&format!(
            "Could not find {} declaration",
            pou_type.start_keyword()
        ))
    })?;
    let declaration_match = captures.get(0).expect("whole declaration match");
    let name = captures
        .name("name")
        .map(|value| value.as_str().to_owned())
        .unwrap_or_default();
    let return_type = captures
        .name("returnType")
        .map(|value| value.as_str().to_owned());
    if pou_type == PouType::Function && return_type.is_none() {
        return Err(parse_error(&format!(
            "Function {name} must have a return type"
        )));
    }

    let var_start_index = first_var_start(remaining);
    let mut variables_string = "";
    let mut body_start_index = declaration_match.end();
    if let Some(var_start) = var_start_index {
        if let Some(last_end_var) = find_last_end_var_index(remaining, var_start) {
            variables_string = &remaining[var_start..last_end_var];
            body_start_index = last_end_var;
        }
    }
    let variables = if variables_string.trim().is_empty() {
        Vec::new()
    } else {
        parse_iec_string_to_variables(variables_string, function_blocks)
            .map_err(|error| parse_error(&error))?
    };

    let end_keyword = pou_type.end_keyword();
    let tail = &remaining[body_start_index..];
    let end_match = keyword_regex(end_keyword)
        .find(tail)
        .ok_or_else(|| parse_error(&format!("Could not find {end_keyword}")))?;
    let raw_body = tail[..end_match.start()].trim();

    let body = match mode {
        BodyMode::Textual | BodyMode::Hybrid => PouBody::text(language, raw_body),
        BodyMode::Graphical => {
            let value: Value = serde_json::from_str(raw_body).map_err(|error| {
                parse_error(&format!("Invalid JSON in graphical body: {error}"))
            })?;
            PouBody { language, value }
        }
    };

    Ok(ParsedPou {
        name,
        pou_type,
        interface: PouInterface {
            return_type,
            variables,
        },
        body,
        documentation,
    })
}

fn extract_documentation(content: &str) -> (String, &str) {
    let Some(captures) = documentation_regex().captures(content) else {
        return (String::new(), content);
    };
    let documentation = captures
        .get(1)
        .map(|value| value.as_str().trim().to_owned())
        .unwrap_or_default();
    let end = captures.get(0).map(|value| value.end()).unwrap_or(0);
    (documentation, &content[end..])
}

fn first_var_start(content: &str) -> Option<usize> {
    var_start_regex().find(content).map(|found| found.start())
}

fn parse_error(message: &str) -> String {
    format!("Parse error: {message}")
}

fn declaration_regex(pou_type: PouType) -> Regex {
    RegexBuilder::new(&format!(
        r"^\s*({})\s+(?P<name>[A-Za-z0-9_]+)(?:\s*:\s*(?P<returnType>[A-Za-z0-9_]+))?",
        pou_type.start_keyword()
    ))
    .case_insensitive(true)
    .build()
    .expect("pinned POU declaration regex")
}

fn documentation_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        RegexBuilder::new(r"^\s*\(\*\s*(.*?)\s*\*\)\s*\n")
            .dot_matches_new_line(true)
            .build()
            .expect("pinned POU documentation regex")
    })
}

fn var_start_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        RegexBuilder::new(
            r"\b(VAR_INPUT|VAR_OUTPUT|VAR_IN_OUT|VAR_EXTERNAL|VAR_TEMP|VAR_GLOBAL|VAR)\b",
        )
        .case_insensitive(true)
        .build()
        .expect("pinned VAR start regex")
    })
}

fn end_var_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| keyword_regex("END_VAR"))
}

fn keyword_regex(keyword: &str) -> Regex {
    RegexBuilder::new(&format!(r"\b{}\b", regex::escape(keyword)))
        .case_insensitive(true)
        .build()
        .expect("pinned IEC keyword regex")
}

#[cfg(test)]
mod tests {
    use super::{
        find_last_end_var_index, parse_graphical_pou, parse_hybrid_pou, parse_textual_pou,
    };
    use crate::plc_model::{PouLanguage, PouType, VariableClass};
    use serde_json::json;

    #[test]
    fn find_last_end_var_matches_openplc() {
        let content = "VAR x : INT; END_VAR VAR y : BOOL; end_var body";
        assert_eq!(
            find_last_end_var_index(content, 0),
            Some(content.to_ascii_lowercase().rfind("end_var").unwrap() + "end_var".len())
        );
        assert_eq!(find_last_end_var_index("VAR x : INT;", 0), None);
    }

    #[test]
    fn parses_openplc_textual_program_and_documentation() {
        let content = "(* This is documented *)\n\nPROGRAM Main\nVAR\n  x : INT;\nEND_VAR\n\nx := 1;\n\nEND_PROGRAM";
        let parsed = parse_textual_pou(content, PouLanguage::St, PouType::Program, &[]).unwrap();
        assert_eq!(parsed.name, "Main");
        assert_eq!(parsed.documentation, "This is documented");
        assert_eq!(parsed.interface.variables.len(), 1);
        assert_eq!(parsed.interface.variables[0].name, "x");
        assert_eq!(
            parsed.interface.variables[0].class,
            Some(VariableClass::Local)
        );
        assert_eq!(parsed.body.value, json!("x := 1;"));
    }

    #[test]
    fn parses_openplc_function_return_and_multiple_var_blocks() {
        let content = "FUNCTION MyFunc : INT\nVAR_INPUT\n  a : INT;\nEND_VAR\nVAR_OUTPUT\n  b : INT;\nEND_VAR\n\nb := a + 1;\n\nEND_FUNCTION";
        let parsed = parse_textual_pou(content, PouLanguage::St, PouType::Function, &[]).unwrap();
        assert_eq!(parsed.interface.return_type.as_deref(), Some("INT"));
        assert_eq!(parsed.interface.variables.len(), 2);
        assert_eq!(parsed.body.value, json!("b := a + 1;"));
    }

    #[test]
    fn preserves_openplc_missing_end_var_behavior() {
        let content = "PROGRAM Main\nVAR x : INT;\nx := 1;\nEND_PROGRAM";
        let parsed = parse_textual_pou(content, PouLanguage::St, PouType::Program, &[]).unwrap();
        assert!(parsed.interface.variables.is_empty());
        assert!(parsed.body.value.as_str().unwrap().contains("VAR x : INT;"));
    }

    #[test]
    fn parses_openplc_hybrid_and_strips_end_keyword() {
        let content = "PROGRAM PyMain\nVAR\n  x : INT;\nEND_VAR\nprint(\"hello\")\nEND_PROGRAM";
        let parsed = parse_hybrid_pou(content, PouLanguage::Python, PouType::Program, &[]).unwrap();
        assert_eq!(parsed.body.value, json!("print(\"hello\")"));
    }

    #[test]
    fn parses_openplc_graphical_json() {
        let content = "PROGRAM LdMain\nVAR\n  x : INT;\nEND_VAR\n\n{\n  \"rungs\": [{\"id\": \"1\", \"nodes\": [], \"edges\": []}]\n}\nEND_PROGRAM";
        let parsed = parse_graphical_pou(content, PouLanguage::Ld, PouType::Program, &[]).unwrap();
        assert_eq!(parsed.body.value["rungs"][0]["id"], "1");
    }

    #[test]
    fn matches_openplc_parser_errors() {
        assert!(parse_textual_pou(
            "x := 1;\nEND_PROGRAM",
            PouLanguage::St,
            PouType::Program,
            &[]
        )
        .unwrap_err()
        .contains("Could not find PROGRAM declaration"));
        assert!(parse_textual_pou(
            "FUNCTION NoReturn\nEND_FUNCTION",
            PouLanguage::St,
            PouType::Function,
            &[]
        )
        .unwrap_err()
        .contains("must have a return type"));
        assert!(parse_graphical_pou(
            "PROGRAM Main\nnot-json\nEND_PROGRAM",
            PouLanguage::Ld,
            PouType::Program,
            &[]
        )
        .unwrap_err()
        .contains("Invalid JSON in graphical body"));
    }
}
