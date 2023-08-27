use anyhow::Context;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let current_dir = std::env::current_dir()?;
    current_dir
        .ends_with("groupcache-pb")
        .then_some(0)
        .context(format!(
            "must be run from the root of the crate, instead was {:#?}",
            current_dir
        ))?;

    tonic_build::configure()
        .out_dir("src/")
        .compile(&["protos/groupcache.proto"], &["protos/"])?;
    Ok(())
}
