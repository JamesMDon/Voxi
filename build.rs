fn main() {
    println!("cargo:rerun-if-changed=assets/voxi.ico");
    println!("cargo:rerun-if-changed=assets/voxi-active.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon("assets/voxi.ico")
        .set_icon_with_id("assets/voxi-active.ico", "2")
        .set("ProductName", "Voxi")
        .set(
            "FileDescription",
            "Lightweight Windows text-to-speech tray app",
        )
        .set("LegalCopyright", "MIT License");
    resource.compile().expect("failed to embed Voxi resources");
}
