use std::env;

fn main() {
    println!("cargo:rerun-if-changed=src/aarch64_i8_dot.c");

    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64") {
        cc::Build::new()
            .file("src/aarch64_i8_dot.c")
            .warnings(false)
            .compile("retrievalkit_aarch64_i8_dot");
    }
}
