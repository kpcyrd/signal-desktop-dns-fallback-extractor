use anyhow::Result;
use log::debug;
use semver::Version;
use sh::sh;

const GIT_URL: &str = "https://github.com/signalapp/signal-desktop";
const MIN_VERSION: Version = Version::new(7, 1, 0);

fn get_versions() -> Result<Vec<Version>> {
    let mut out = String::new();
    sh!(git "ls-remote" "--tags" {GIT_URL} > {&mut out});

    let mut versions = Vec::new();
    for line in out.lines() {
        debug!("git ls-remote line={line:?}");
        let Some((_hash, version)) = line.split_once("\trefs/tags/v") else {
            continue;
        };
        let Ok(version) = Version::parse(version) else {
            continue;
        };
        if !version.pre.is_empty() {
            continue;
        }

        versions.push(version);
    }

    Ok(versions)
}

#[tokio::main]
async fn main() -> Result<()> {
    let versions = get_versions()?;
    for version in versions {
        if version < MIN_VERSION {
            continue;
        }
        println!("version={version:?}");
    }

    Ok(())
}
