fn main() {
    println!("cargo:rerun-if-changed=native/strucpp_bridge.cpp");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        return;
    }
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .warnings(true)
        .file("native/strucpp_bridge.cpp")
        .compile("open_rust_plc_strucpp_bridge");
}
