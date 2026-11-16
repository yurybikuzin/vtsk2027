fn main() {
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match target.as_str() {
        "linux" => {
            println!("cargo:rustc-link-lib=dylib=openblas");
        }
        "macos" => {
            println!("cargo:rustc-link-lib=framework=Accelerate");
        }
        _ => {}
    }
}
