fn main() {
    // Windows gives a program's main thread 1 MB of stack; Linux and macOS
    // give it 8 MB. Tauri sets up every command on the main thread, so the
    // Windows default leaves the least headroom of the three. Use the same
    // 8 MB everywhere. (The commands are also written to need very little;
    // see `bounded` in src/commands.rs.)
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os == "windows" {
        let stack = 8 * 1024 * 1024;
        if env == "msvc" {
            println!("cargo:rustc-link-arg-bins=/STACK:{stack}");
        } else {
            println!("cargo:rustc-link-arg-bins=-Wl,--stack,{stack}");
        }
    }

    tauri_build::build()
}
