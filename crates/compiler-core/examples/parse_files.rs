use std::env;
use std::fs;
use std::path::Path;

use open_rust_plc_compiler::frontend::parse;

fn main() {
    let mut passed = 0usize;
    let mut failed = 0usize;
    for arg in env::args().skip(1) {
        let path = Path::new(&arg);
        let source = match fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) => {
                failed += 1;
                println!("FAIL\t{}\tread: {error}", path.display());
                continue;
            }
        };
        match parse(&source, &path.to_string_lossy()) {
            Ok(_) => {
                passed += 1;
                println!("OK\t{}", path.display());
            }
            Err(errors) => {
                failed += 1;
                let message = errors
                    .first()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "parse error".to_owned());
                println!("FAIL\t{}\t{}", path.display(), message.replace('\n', " "));
            }
        }
    }
    eprintln!(
        "SUMMARY passed={passed} failed={failed} total={}",
        passed + failed
    );
    if failed > 0 {
        std::process::exit(2);
    }
}
