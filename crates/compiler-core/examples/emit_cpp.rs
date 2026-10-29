use std::env;
use std::fs;
use std::path::PathBuf;

use open_rust_plc_compiler::backend::generate_cpp;
use open_rust_plc_compiler::frontend::parse;
use open_rust_plc_compiler::md5::{defines_h, md5_hex};
use open_rust_plc_compiler::stdlib::with_iec_standard_fb;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let source_path = PathBuf::from(args.next().ok_or("usage: emit_cpp INPUT.st OUTPUT_DIR")?);
    let output_dir = PathBuf::from(args.next().ok_or("usage: emit_cpp INPUT.st OUTPUT_DIR")?);
    if args.next().is_some() {
        return Err("usage: emit_cpp INPUT.st OUTPUT_DIR".into());
    }
    let source = fs::read_to_string(&source_path)?;
    let file = source_path.to_string_lossy();
    let ast = parse(&source, &file).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let ast = with_iec_standard_fb(ast).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| error.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let mut generated = generate_cpp(&ast)?;
    let md5 = md5_hex(source.as_bytes());
    generated.debug_map.md5 = md5.clone();
    if output_dir.exists() {
        fs::remove_dir_all(&output_dir)?;
    }
    fs::create_dir_all(&output_dir)?;
    fs::write(output_dir.join("generated.hpp"), generated.header_code)?;
    fs::write(output_dir.join("defines.h"), defines_h(&md5))?;
    fs::write(
        output_dir.join("debug_map.json"),
        serde_json::to_string_pretty(&generated.debug_map)?,
    )?;
    for file in generated.cpp_files {
        fs::write(output_dir.join(file.name), file.content)?;
    }
    Ok(())
}
