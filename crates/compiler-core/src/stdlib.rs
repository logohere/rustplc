use crate::frontend::ast::CompilationUnit;
use crate::frontend::{parse, ParseError};

const IEC_STANDARD_FB_SOURCES: [(&str, &str); 5] = [
    (
        "iec-standard-fb/bistable.st",
        include_str!("../stdlib/iec-standard-fb/bistable.st"),
    ),
    (
        "iec-standard-fb/counter.st",
        include_str!("../stdlib/iec-standard-fb/counter.st"),
    ),
    (
        "iec-standard-fb/edge_detection.st",
        include_str!("../stdlib/iec-standard-fb/edge_detection.st"),
    ),
    (
        "iec-standard-fb/sema.st",
        include_str!("../stdlib/iec-standard-fb/sema.st"),
    ),
    (
        "iec-standard-fb/timer.st",
        include_str!("../stdlib/iec-standard-fb/timer.st"),
    ),
];

pub fn iec_standard_fb_unit() -> Result<CompilationUnit, Vec<ParseError>> {
    let mut merged = CompilationUnit::default();
    let mut errors = Vec::new();
    for (name, source) in IEC_STANDARD_FB_SOURCES {
        match parse(source, name) {
            Ok(unit) => merged.extend(unit),
            Err(mut source_errors) => errors.append(&mut source_errors),
        }
    }
    if errors.is_empty() {
        Ok(merged)
    } else {
        Err(errors)
    }
}

pub fn with_iec_standard_fb(
    mut project: CompilationUnit,
) -> Result<CompilationUnit, Vec<ParseError>> {
    project.extend(iec_standard_fb_unit()?);
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::iec_standard_fb_unit;

    #[test]
    fn bundled_standard_fb_sources_parse_as_owned_rust_ast() {
        let unit = iec_standard_fb_unit().unwrap();
        let names = unit
            .function_blocks
            .iter()
            .map(|fb| fb.name.to_ascii_uppercase())
            .collect::<std::collections::HashSet<_>>();
        for expected in [
            "SR", "RS", "R_TRIG", "F_TRIG", "CTU", "CTD", "CTUD", "TP", "TON", "TOF", "SEMA",
        ] {
            assert!(names.contains(expected), "missing bundled FB {expected}");
        }
    }
}
