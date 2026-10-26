fn main() {
    assert!(
        std::path::Path::new("build/icons/phaselimiter-256.png").is_file(),
        "Generate application icons first: python3 script/build-icons.py"
    );
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-env-changed=RC_PATH");
    println!("cargo:rerun-if-changed=build/icons/phaselimiter.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("build/icons/phaselimiter.ico")
            .compile()
            .expect("compile Windows application icon");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        // Match MA_NO_RUNTIME_LINKING in .cargo/config.toml. These are system
        // frameworks, not additional libraries shipped inside the application.
        for framework in ["CoreFoundation", "CoreAudio", "AudioUnit", "AudioToolbox"] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
    }
}
