use anyhow::anyhow;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let current_dir = std::env::current_dir()?;
    if !current_dir.ends_with("groupcache-pb") {
        return Err(anyhow!(
            "must be run from the root of the crate, instead was {:#?}",
            current_dir
        )
        .into());
    }

    tonic_build::configure()
        .out_dir("src/")
        .compile_protos(&["protos/groupcache.proto"], &["protos/"])?;
    Ok(())
}
