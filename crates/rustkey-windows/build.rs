fn main() {
    println!("cargo:rerun-if-changed=rustkey.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    embed_resource::compile("rustkey.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap_or_else(|error| println!("cargo:warning=icon resource not embedded: {error}"));
}
