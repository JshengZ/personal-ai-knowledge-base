fn main() {
    println!("cargo:rerun-if-changed=../package.json");
    let package: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("../package.json").expect("application version source"),
    )
    .expect("valid package.json");
    let version = package["version"].as_str().expect("application version");
    assert_eq!(
        version,
        std::env::var("CARGO_PKG_VERSION").unwrap(),
        "Run node scripts/sync-version.mjs after changing the application version"
    );
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set(
                "FileDescription",
                "Personal Knowledge Engine (service / MCP bridge)",
            )
            .set("ProductName", "Personal Knowledge")
            .set("InternalName", "knowledge-engine")
            .set("OriginalFilename", "knowledge-engine.exe")
            .set("FileVersion", version)
            .set("ProductVersion", version);
        resource.compile().expect("Windows engine version resource");
    }
}
