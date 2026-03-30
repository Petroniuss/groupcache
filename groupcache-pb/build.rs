fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Generated code is checked in so that users don't need protoc installed.
    // To regenerate after updating the .proto or bumping tonic/prost versions,
    // change `false` to `true` below and run `cargo build -p groupcache-pb`.
    if false {
        tonic_prost_build::configure()
            .out_dir("src/")
            .compile_protos(&["protos/groupcache.proto"], &["protos/"])?;
    }

    Ok(())
}
