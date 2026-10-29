use open_rust_plc_compat_core::program_api::ProgramApi;

#[test]
fn missing_program_is_an_error() {
    assert!(ProgramApi::load("missing-openplc-program.so").is_err());
}
