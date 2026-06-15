fn main() {
    embed_resource::compile("embed/steps.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
