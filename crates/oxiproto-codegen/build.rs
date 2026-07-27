//! Compile the test fixture protos in `tests/fixtures` into `OUT_DIR`.
//!
//! This deliberately avoids `prost_build::Config::compile_protos`, which shells
//! out to a `protoc` executable: that made OxiProto — a project whose entire
//! purpose is removing the `protoc` prerequisite — itself unbuildable on a
//! machine without `protoc` installed. `protox` parses the fixtures in-process
//! and `compile_fds` generates Rust straight from the resulting descriptor set,
//! so no external process is involved.
//!
//! `protox` rather than the sibling `oxiproto-build` (the natural dogfooding
//! choice) because `oxiproto-build` optionally depends on *this* crate for its
//! `native-codegen` feature; build-depending on it here would risk a
//! dependency cycle. `oxiproto-build` reaches for `protox` on exactly the same
//! path, so the descriptors are identical either way.

fn main() {
    let proto_dir = "tests/fixtures";
    let files = [
        "scalars.proto",
        "nested.proto",
        "oneof_map.proto",
        "services.proto",
    ];
    let protos: Vec<String> = files.iter().map(|f| format!("{proto_dir}/{f}")).collect();

    for proto in &protos {
        println!("cargo:rerun-if-changed={proto}");
    }

    // No `--experimental_allow_proto3_optional` equivalent is needed here: that
    // flag only ever existed to unlock proto3 `optional` on protoc releases
    // older than 3.15, and protox supports field presence natively.
    let fds = protox::compile(&protos, [proto_dir]).expect("protox failed to parse fixture protos");

    prost_build::Config::new()
        .compile_fds(fds)
        .expect("prost-build failed to generate Rust from the fixture descriptors");
}
