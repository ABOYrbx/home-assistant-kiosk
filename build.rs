fn main() {
    // The Slint testing backend needs debug info to resolve element IDs. This
    // covers both `ui/main.slint` and `slint::slint!` fixtures compiled by the
    // integration tests.
    println!("cargo:rustc-env=SLINT_EMIT_DEBUG_INFO=1");

    let config = slint_build::CompilerConfiguration::new().with_debug_info(true);
    slint_build::compile_with_config("ui/main.slint", config).expect("Slint compile failed");
}
