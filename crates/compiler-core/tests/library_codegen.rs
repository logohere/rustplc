use open_rust_plc_compiler::backend::generate_cpp_with_libraries;
use open_rust_plc_compiler::frontend::parse;
use open_rust_plc_compiler::library::StlibArchive;

#[test]
fn closed_source_stlib_manifest_and_chunks_compile_without_sources() {
    let stlib = r#"{
      "formatVersion":1,
      "manifest":{
        "name":"demo","version":"1.0.0","namespace":"demo",
        "functions":[{"name":"ADDONE","returnType":"INT","parameters":[{"name":"A","type":"INT","direction":"input"}]}],
        "functionBlocks":[{"name":"COUNTER","inputs":[{"name":"EN","type":"BOOL"}],"outputs":[{"name":"Q","type":"INT"}],"inouts":[]}],
        "types":[{"name":"PAIR","kind":"struct","fields":[{"name":"X","type":"INT"}]}],
        "globals":[],"headers":[],"isBuiltin":false
      },
      "chunks":[
        {"name":"PAIR","kind":"type","header":"struct PAIR {\n    IEC_INT X{};\n};\nusing IEC_PAIR = PAIR;\n","cpp":"","deps":[]},
        {"name":"COUNTER","kind":"functionBlock","header":"class COUNTER { public: IEC_BOOL EN; IEC_INT Q; IEC_BOOL ENO = true; COUNTER(); void operator()(); };\n","cpp":"COUNTER::COUNTER() {}\nvoid COUNTER::operator()() { Q = Q + 1; }\n","deps":[]},
        {"name":"ADDONE","kind":"function","header":"IEC_INT ADDONE(IEC_INT A);\n","cpp":"IEC_INT ADDONE(IEC_INT A) { return A + 1; }\n","deps":[]},
        {"name":"UNUSED","kind":"function","header":"IEC_INT UNUSED(IEC_INT A);\n","cpp":"IEC_INT UNUSED(IEC_INT A) { return A; }\n","deps":[]}
      ],
      "dependencies":[]
    }"#;
    let archive = StlibArchive::parse(stlib, "demo.stlib").unwrap();
    assert!(archive.sources.is_empty());

    let source = r#"
PROGRAM main
VAR
    C : COUNTER;
    P : PAIR;
    R : INT;
END_VAR
C(EN := TRUE);
R := ADDONE(C.Q);
P.X := R;
END_PROGRAM
CONFIGURATION Config0
RESOURCE Res0 ON PLC
TASK task0(INTERVAL := T#20ms, PRIORITY := 1);
PROGRAM instance0 WITH task0 : main;
END_RESOURCE
END_CONFIGURATION
"#;
    let ast = parse(source, "library-user.st").unwrap();
    let generated = generate_cpp_with_libraries(&ast, &[archive], source).unwrap();

    assert!(generated.header_code.contains("struct PAIR"));
    assert!(generated.header_code.contains("class COUNTER"));
    assert!(generated.header_code.contains("IEC_INT ADDONE(IEC_INT A);"));
    assert!(!generated.header_code.contains("UNUSED("));
    assert!(generated.header_code.contains("COUNTER C;"));
    assert!(generated.header_code.contains("PAIR P;"));

    let program = generated
        .cpp_files
        .iter()
        .find(|file| file.name == "pou_MAIN.cpp")
        .unwrap();
    assert!(program.content.contains("C.EN = true;"));
    assert!(program.content.contains("C();"));
    assert!(program.content.contains("R = ADDONE(C.Q);"));
    assert!(program.content.contains("P.X = R;"));

    let library_files = generated
        .cpp_files
        .iter()
        .filter(|file| file.name.starts_with("lib_demo_"))
        .map(|file| file.name.as_str())
        .collect::<Vec<_>>();
    assert!(library_files.contains(&"lib_demo_counter.cpp"));
    assert!(library_files.contains(&"lib_demo_addone.cpp"));
    assert!(!library_files.contains(&"lib_demo_unused.cpp"));
}
