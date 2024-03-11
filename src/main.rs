use anyhow::{anyhow, bail, Context as _, Result};
use asar::AsarReader;
use clap::{ArgAction, Parser};
use env_logger::Env;
use log::{debug, error, info};
use semver::Version;
use sh::sh;
use std::fs;
use std::io::BufReader;
use std::io::Read;
use std::path::PathBuf;
use std::process::Command;
use std::str;

const GIT_URL: &str = "https://github.com/signalapp/signal-desktop";
const MIN_VERSION: Version = Version::new(7, 1, 0);
const DNS_FALLBACK_PATH: &str = "build/dns-fallback.json";

#[derive(Debug, Parser)]
struct Args {
    #[arg(short, long, action(ArgAction::Count))]
    pub verbose: u8,
    #[arg(short = 'n', long)]
    pub dry_run: bool,
    #[arg(long)]
    pub deb: Option<PathBuf>,
    #[arg(long)]
    pub asar: Option<PathBuf>,
}

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

async fn download(url: &str) -> Result<Vec<u8>> {
    info!("Downloading from url: {url:?}");
    let bytes = reqwest::get(url).await?.error_for_status()?.bytes().await?;
    info!("Received {} bytes", bytes.len());
    Ok(bytes.to_vec())
}

fn extract_from_asar(bytes: &[u8]) -> Result<String> {
    let reader = AsarReader::new(bytes, None).context("Failed to open asar")?;
    for (path, file) in reader.files() {
        if path.to_str() == Some(DNS_FALLBACK_PATH) {
            let data = String::from_utf8(file.data().to_vec())?;
            debug!("Found json in asar: {data:?}");
            return Ok(data);
        }
    }
    bail!("Failed to find {DNS_FALLBACK_PATH:?} in app.asar")
}

fn extract_from_deb(bytes: &[u8]) -> Result<String> {
    let mut archive = ar::Archive::new(bytes);

    while let Some(entry) = archive.next_entry() {
        let entry = entry?;
        let name = String::from_utf8(entry.header().identifier().to_vec());
        debug!("Found file in .deb: {name:?}");
        if name.as_deref() != Ok("data.tar.xz") {
            continue;
        }

        let mut reader = BufReader::new(entry);
        let mut buf = Vec::new();
        info!("Decompressing from deb...");
        lzma_rs::xz_decompress(&mut reader, &mut buf)?;

        let mut tar = tar::Archive::new(&buf[..]);
        for entry in tar.entries()? {
            let mut entry = entry?;
            let header = entry.header();
            debug!("Found entry in control tar: {:?}", header.path());

            let Ok(path) = header.path() else { continue };
            let Some(file_name) = path.file_name() else {
                continue;
            };
            if file_name.to_str() != Some("app.asar") {
                continue;
            }

            info!("Found asar: {path:?}");
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            return extract_from_asar(&bytes).context("Failed to parse from asar in .deb");
        }
    }

    bail!("Could not find app.asar in .deb")
}

async fn prepare_release(version: &Version, dry_run: bool) -> Result<()> {
    let version = version.to_string();
    let existing = Command::new("git").args(["show", &version]).output()?;
    if existing.status.success() {
        info!("Tag already exists: {version}");
        return Ok(());
    }

    let url = format!("https://updates.signal.org/desktop/apt/pool/s/signal-desktop/signal-desktop_{version}_amd64.deb");
    let bytes = download(&url)
        .await
        .with_context(|| anyhow!("Failed to download {url:?}"))?;

    let json = extract_from_deb(&bytes)?;
    info!("json = {} bytes", json.len());

    info!("Creating tag for version: {version}");
    Command::new("git").args(["branch", "-D", "tmp"]).output()?;

    debug!("git checkout");
    sh!(git checkout "--orphan" tmp);
    debug!("git reset");
    sh!(git reset ".");
    debug!("creating dns-fallback.json");
    fs::write("dns-fallback.json", json.as_bytes()).context("Failed to write dns-fallback.json")?;
    debug!("git add dns-fallback.json");
    sh!(git add "dns-fallback.json");
    debug!("git commit");
    sh!(git commit "-m" {&version});
    debug!("git tag");
    sh!(git tag {&version});
    sh!(git reset main);
    sh!(git checkout main);

    if !dry_run {
        info!("Pushing tag...");
        sh!(git push origin {&version});
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let log_level = match args.verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    env_logger::init_from_env(Env::default().default_filter_or(log_level));

    let mut had_error = false;

    if let Some(path) = args.deb {
        let bytes = fs::read(path)?;
        let json = extract_from_deb(&bytes)?;
        print!("{json}");
    } else if let Some(path) = args.asar {
        let bytes = fs::read(path)?;
        let json = extract_from_asar(&bytes)?;
        print!("{json}");
    } else {
        let versions = get_versions()?;
        for version in versions {
            if version < MIN_VERSION {
                continue;
            }

            if let Err(err) = prepare_release(&version, args.dry_run).await {
                error!("Failed to prepare release for {version}: {err:#}");
                had_error = true;
            }
        }
    }

    if !had_error {
        Ok(())
    } else {
        bail!("An error occured");
    }
}
