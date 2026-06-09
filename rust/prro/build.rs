// `sqlx::migrate!()` is a proc-macro that embeds the contents of `migrations/`
// at compile time. Cargo does not, by default, re-run a proc-macro when a file
// inside a watched directory changes, so keep the migration directory watched.
//
// M2/W3 also generates tonic bindings for the production DPS gRPC proto. Use a
// vendored protoc binary so CI and developer machines do not depend on a
// system-level protobuf compiler being installed.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-changed=proto/fiscal_server.proto");

    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile_protos(&["proto/fiscal_server.proto"], &["proto"])?;

    Ok(())
}
