fn main() {
    println!("cargo:rustc-check-cfg=cfg(edition_store)");
    let edition = std::env::var("PROCESSFOX_EDITION").unwrap_or_else(|_| "community".into());
    if edition == "store" {
        println!("cargo:rustc-cfg=edition_store");
    }
    println!("cargo:rerun-if-env-changed=PROCESSFOX_EDITION");
    tauri_build::build()
}
