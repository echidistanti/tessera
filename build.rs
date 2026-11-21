fn main() {
    // Embeds assets/tessera.ico as icon resource 1 (used for the .exe and the tray).
    println!("cargo:rerun-if-changed=assets/tessera.rc");
    println!("cargo:rerun-if-changed=assets/tessera.ico");
    embed_resource::compile("assets/tessera.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
