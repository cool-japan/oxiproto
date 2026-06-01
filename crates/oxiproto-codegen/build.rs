fn main() {
    let proto_dir = "tests/fixtures";
    let files = [
        "scalars.proto",
        "nested.proto",
        "oneof_map.proto",
        "services.proto",
    ];
    let protos: Vec<String> = files.iter().map(|f| format!("{proto_dir}/{f}")).collect();
    let proto_refs: Vec<&str> = protos.iter().map(|s| s.as_str()).collect();

    // Some bundled `protoc` releases (< 3.15) gate proto3 `optional` fields
    // behind an experimental flag. Pass it explicitly so the fixture protos
    // (which use `optional`) compile regardless of the installed protoc.
    prost_build::Config::new()
        .protoc_arg("--experimental_allow_proto3_optional")
        .compile_protos(&proto_refs, &[proto_dir])
        .expect("prost-build failed to compile fixture protos");
}
