fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    // No-op unless the target is Windows; a failed embed must fail the build.
    embed_resource::compile("assets/icon.ico", embed_resource::NONE)
        .manifest_required()
        .expect("embed assets/icon.ico into the executable");
}
