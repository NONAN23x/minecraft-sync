use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use zip::ZipArchive;

const DEFAULT_MANIFEST_URL: &str =
    "https://github.com/NONAN23x/minecraft-sync/releases/latest/download/manifest.json";

#[derive(Debug, Deserialize)]
struct Manifest {
    version: String,
    minecraft_version: String,
    release_tag: String,
    assets: Assets,
}

#[derive(Debug, Deserialize)]
struct Assets {
    mods: ReleaseAsset,
    resourcepacks: ReleaseAsset,
    shaderpacks: ReleaseAsset,
    fabric_installer: Option<ReleaseAsset>,
}

#[derive(Debug, Deserialize, Clone)]
struct ReleaseAsset {
    url: String,
    sha256: String,
    size: u64,
}

#[derive(Debug)]
struct Args {
    manifest_url: String,
    minecraft_dir: Option<PathBuf>,
    skip_fabric: bool,
    skip_mods: bool,
    skip_resourcepacks: bool,
    skip_shaderpacks: bool,
}

#[derive(Debug)]
struct FolderState {
    target: PathBuf,
    backup: Option<PathBuf>,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("[!] {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = parse_args()?;
    let client = Client::builder()
        .user_agent("minecraft-sync-installer")
        .build()
        .context("failed to build HTTP client")?;

    println!("Minecraft Sync Installer");
    println!("========================");

    let manifest = download_manifest(&client, &args.manifest_url)?;
    println!(
        "[+] Release {} for Minecraft {}",
        manifest.release_tag, manifest.minecraft_version
    );

    let minecraft_dir = match args.minecraft_dir {
        Some(path) => path,
        None => detect_minecraft_dir()?,
    };

    if !minecraft_dir.exists() {
        bail!(
            "Minecraft directory does not exist: {}",
            minecraft_dir.display()
        );
    }

    println!("[+] Using Minecraft directory: {}", minecraft_dir.display());

    let temp_dir = TempDir::new().context("failed to create temp directory")?;

    if !args.skip_fabric {
        if let Some(asset) = &manifest.assets.fabric_installer {
            install_fabric(
                &client,
                &temp_dir,
                asset,
                &minecraft_dir,
                &manifest.minecraft_version,
            )?;
        } else {
            println!("[+] No Fabric installer asset present in manifest, skipping");
        }
    }

    let mut changed_folders = Vec::new();

    sync_pack(
        &client,
        &temp_dir,
        "mods",
        &manifest.assets.mods,
        &minecraft_dir,
        args.skip_mods,
        &mut changed_folders,
    )?;
    sync_pack(
        &client,
        &temp_dir,
        "resourcepacks",
        &manifest.assets.resourcepacks,
        &minecraft_dir,
        args.skip_resourcepacks,
        &mut changed_folders,
    )?;
    sync_pack(
        &client,
        &temp_dir,
        "shaderpacks",
        &manifest.assets.shaderpacks,
        &minecraft_dir,
        args.skip_shaderpacks,
        &mut changed_folders,
    )?;

    println!();
    println!(
        "[✓] Sync completed for manifest version {}",
        manifest.version
    );
    Ok(())
}

fn parse_args() -> Result<Args> {
    let mut args = Args {
        manifest_url: DEFAULT_MANIFEST_URL.to_string(),
        minecraft_dir: None,
        skip_fabric: false,
        skip_mods: false,
        skip_resourcepacks: false,
        skip_shaderpacks: false,
    };

    let mut iter = env::args_os().skip(1);

    while let Some(arg) = iter.next() {
        match arg.to_string_lossy().as_ref() {
            "--manifest-url" => {
                let value = next_value(&mut iter, "--manifest-url")?;
                args.manifest_url = value
                    .into_string()
                    .map_err(|_| anyhow!("manifest URL must be valid UTF-8"))?;
            }
            "--minecraft-dir" => {
                let value = next_value(&mut iter, "--minecraft-dir")?;
                args.minecraft_dir = Some(PathBuf::from(value));
            }
            "--skip-fabric" => args.skip_fabric = true,
            "--skip-mods" => args.skip_mods = true,
            "--skip-resourcepacks" => args.skip_resourcepacks = true,
            "--skip-shaderpacks" => args.skip_shaderpacks = true,
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => bail!("unknown argument: {other}"),
        }
    }

    Ok(args)
}

fn next_value(iter: &mut impl Iterator<Item = OsString>, flag: &str) -> Result<OsString> {
    iter.next()
        .ok_or_else(|| anyhow!("missing value for {flag}"))
}

fn print_help() {
    println!("Usage: minecraft-sync [options]");
    println!("  --manifest-url <url>   Override manifest URL");
    println!("  --minecraft-dir <dir>  Override detected Minecraft directory");
    println!("  --skip-fabric          Skip Fabric installation");
    println!("  --skip-mods            Skip mods sync");
    println!("  --skip-resourcepacks   Skip resourcepacks sync");
    println!("  --skip-shaderpacks     Skip shaderpacks sync");
}

fn download_manifest(client: &Client, manifest_url: &str) -> Result<Manifest> {
    println!("[+] Fetching manifest: {manifest_url}");
    let response = client
        .get(manifest_url)
        .send()
        .with_context(|| format!("failed to download manifest from {manifest_url}"))?
        .error_for_status()
        .with_context(|| format!("manifest request failed for {manifest_url}"))?;

    response
        .json::<Manifest>()
        .context("failed to parse manifest JSON")
}

fn detect_minecraft_dir() -> Result<PathBuf> {
    let os = env::consts::OS;
    match os {
        "windows" => {
            let appdata = env::var_os("APPDATA").ok_or_else(|| anyhow!("APPDATA is not set"))?;
            Ok(PathBuf::from(appdata).join(".minecraft"))
        }
        "macos" => Ok(home_dir()?.join("Library/Application Support/minecraft")),
        "linux" => Ok(home_dir()?.join(".minecraft")),
        other => bail!("unsupported OS: {other}"),
    }
}

fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("HOME is not set"))
}

fn install_fabric(
    client: &Client,
    temp_dir: &TempDir,
    asset: &ReleaseAsset,
    minecraft_dir: &Path,
    minecraft_version: &str,
) -> Result<()> {
    println!("[+] Installing Fabric");
    let jar_path = download_asset(client, temp_dir, "fabric-installer.jar", asset)?;

    let output = Command::new("java")
        .arg("-jar")
        .arg(&jar_path)
        .arg("client")
        .arg("-dir")
        .arg(minecraft_dir)
        .arg("-mcversion")
        .arg(minecraft_version)
        .output()
        .context("failed to launch Java for Fabric installer")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Fabric installer failed: {stderr}");
    }

    println!("[✓] Fabric installed");
    Ok(())
}

fn sync_pack(
    client: &Client,
    temp_dir: &TempDir,
    folder_name: &str,
    asset: &ReleaseAsset,
    minecraft_dir: &Path,
    skip: bool,
    changed_folders: &mut Vec<FolderState>,
) -> Result<()> {
    if skip {
        println!("[+] Skipping {folder_name}");
        return Ok(());
    }

    println!("[+] Syncing {folder_name}");
    let archive_path = download_asset(client, temp_dir, &format!("{folder_name}.zip"), asset)?;
    let target_dir = minecraft_dir.join(folder_name);

    let backup = backup_folder(&target_dir)?;
    changed_folders.push(FolderState {
        target: target_dir.clone(),
        backup,
    });

    if let Err(error) = extract_zip(&archive_path, &target_dir) {
        rollback(changed_folders)?;
        return Err(error).with_context(|| format!("failed while syncing {folder_name}"));
    }

    println!("[✓] Synced {folder_name}");
    Ok(())
}

fn download_asset(
    client: &Client,
    temp_dir: &TempDir,
    file_name: &str,
    asset: &ReleaseAsset,
) -> Result<PathBuf> {
    println!("[+] Downloading {file_name}");
    let response = client
        .get(&asset.url)
        .send()
        .with_context(|| format!("failed to download asset {}", asset.url))?
        .error_for_status()
        .with_context(|| format!("asset request failed for {}", asset.url))?;

    let bytes = response.bytes().context("failed to read asset body")?;
    if bytes.len() as u64 != asset.size {
        bail!(
            "size mismatch for {file_name}: expected {}, got {}",
            asset.size,
            bytes.len()
        );
    }

    let digest = hex_sha256(bytes.as_ref());
    if digest != asset.sha256.to_ascii_lowercase() {
        bail!("sha256 mismatch for {file_name}");
    }

    let file_path = temp_dir.path().join(file_name);
    fs::write(&file_path, &bytes)
        .with_context(|| format!("failed to write temporary asset {}", file_path.display()))?;
    Ok(file_path)
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    format!("{digest:x}")
}

fn backup_folder(target_dir: &Path) -> Result<Option<PathBuf>> {
    if !target_dir.exists() {
        return Ok(None);
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_secs();
    let backup_path = target_dir.with_file_name(format!(
        "{}.bak.{timestamp}",
        target_dir
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow!("invalid target folder name"))?
    ));

    fs::rename(target_dir, &backup_path).with_context(|| {
        format!(
            "failed to create backup {} -> {}",
            target_dir.display(),
            backup_path.display()
        )
    })?;

    println!("[+] Backup created: {}", backup_path.display());
    Ok(Some(backup_path))
}

fn extract_zip(archive_path: &Path, target_dir: &Path) -> Result<()> {
    fs::create_dir_all(target_dir)
        .with_context(|| format!("failed to create {}", target_dir.display()))?;

    let file = File::open(archive_path)
        .with_context(|| format!("failed to open archive {}", archive_path.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("failed to read archive {}", archive_path.display()))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .context("failed to access zip entry")?;
        let enclosed_name = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("archive entry escapes target directory"))?
            .to_owned();
        let output_path = target_dir.join(enclosed_name);

        if entry.name().ends_with('/') {
            fs::create_dir_all(&output_path)
                .with_context(|| format!("failed to create directory {}", output_path.display()))?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let mut output = File::create(&output_path)
            .with_context(|| format!("failed to create {}", output_path.display()))?;
        io::copy(&mut entry, &mut output)
            .with_context(|| format!("failed to write {}", output_path.display()))?;
    }

    Ok(())
}

fn rollback(states: &[FolderState]) -> Result<()> {
    println!("[!] Rolling back partial changes");
    for state in states.iter().rev() {
        if state.target.exists() {
            fs::remove_dir_all(&state.target)
                .with_context(|| format!("failed to remove {}", state.target.display()))?;
        }

        if let Some(backup) = &state.backup {
            if backup.exists() {
                fs::rename(backup, &state.target).with_context(|| {
                    format!(
                        "failed to restore backup {} -> {}",
                        backup.display(),
                        state.target.display()
                    )
                })?;
            }
        }
    }

    Ok(())
}
