fn main() {
    // Embeds assets/tessera.ico as icon resource 1 (used for the .exe and the tray) and the
    // manifest that gives the options panel the current visual style.
    println!("cargo:rerun-if-changed=assets/tessera.rc");
    println!("cargo:rerun-if-changed=assets/tessera.ico");
    println!("cargo:rerun-if-changed=assets/tessera.manifest");
    embed_resource::compile("assets/tessera.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
