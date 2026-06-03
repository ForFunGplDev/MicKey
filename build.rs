fn main() {
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:MicKey.exe.manifest");

    embed_resource::compile("MicKey.rc", embed_resource::NONE);
}