fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        // Match MA_NO_RUNTIME_LINKING in .cargo/config.toml. These are system
        // frameworks, not additional libraries shipped inside the application.
        for framework in ["CoreFoundation", "CoreAudio", "AudioUnit", "AudioToolbox"] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
    }
}
