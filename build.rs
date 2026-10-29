fn main() {
    println!("cargo:rerun-if-changed=res");
    embed_resource::compile("res/nab.rc", embed_resource::NONE).manifest_required().unwrap();
}
