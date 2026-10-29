use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use open_rust_plc_compat_core::program_build::{build, BuildError, BuildSpec};

fn test_root() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("rustplc-build-{stamp}"))
}

#[test]
fn failed_build_preserves_the_prior_artifact() {
    let root = test_root();
    let generated = root.join("generated");
    let include = root.join("include");
    fs::create_dir_all(&generated).unwrap();
    fs::create_dir_all(&include).unwrap();
    fs::write(generated.join("generated.hpp"), b"").unwrap();
    fs::write(generated.join("generated.cpp"), b"").unwrap();
    fs::write(include.join("iec_types.hpp"), b"").unwrap();
    fs::write(root.join("runtime.cpp"), b"").unwrap();
    fs::write(root.join("reset.cpp"), b"").unwrap();
    let output = root.join(if cfg!(target_os = "macos") {
        "plc.dylib"
    } else {
        "plc.so"
    });
    fs::write(&output, b"prior-good-artifact").unwrap();
    let result = build(&BuildSpec {
        compiler: PathBuf::from("false"),
        generated_dir: generated,
        runtime_include: include,
        runtime_shim: root.join("runtime.cpp"),
        reset_shim: root.join("reset.cpp"),
        output: output.clone(),
    });
    if cfg!(unix) {
        assert!(matches!(result, Err(BuildError::CompilerFailed(_, _))));
    } else {
        assert!(matches!(result, Err(BuildError::Unsupported)));
    }
    assert_eq!(fs::read(output).unwrap(), b"prior-good-artifact");
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn successful_library_without_an_abi_cannot_replace_output() {
    let root = test_root();
    let generated = root.join("generated");
    let include = root.join("include");
    fs::create_dir_all(&generated).unwrap();
    fs::create_dir_all(&include).unwrap();
    for path in [
        generated.join("generated.hpp"),
        generated.join("generated.cpp"),
        include.join("iec_types.hpp"),
        root.join("runtime.cpp"),
        root.join("reset.cpp"),
    ] {
        fs::write(path, b"").unwrap();
    }
    let output = root.join("plc.so");
    fs::write(&output, b"prior-good-artifact").unwrap();
    let result = build(&BuildSpec {
        compiler: PathBuf::from("c++"),
        generated_dir: generated,
        runtime_include: include,
        runtime_shim: root.join("runtime.cpp"),
        reset_shim: root.join("reset.cpp"),
        output: output.clone(),
    });
    assert!(
        matches!(result, Err(BuildError::InvalidLibrary(_))),
        "expected ABI validation failure, got {result:?}"
    );
    assert_eq!(fs::read(output).unwrap(), b"prior-good-artifact");
    fs::remove_dir_all(root).unwrap();
}
