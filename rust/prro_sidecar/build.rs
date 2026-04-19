fn main() {
    // Use vendored protoc so the build works without a system protoc install.
    let protoc = protoc_bin_vendored::protoc_bin_path().unwrap();
    std::env::set_var("PROTOC", protoc);

    println!("cargo:rerun-if-changed=proto/check.proto");
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/check.proto"], &["proto/"])
        .expect("tonic_build failed — is proto/check.proto present?");
}
