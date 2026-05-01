<div align="center">

# Minecraft Sync 🚀

![Minecraft Version](https://img.shields.io/badge/Minecraft-26.1.2-green?style=for-the-badge&logo=minecraft)
![License](https://img.shields.io/github/license/NONAN23x/minecraft-sync?style=for-the-badge)
![Stars](https://img.shields.io/github/stars/NONAN23x/minecraft-sync?style=for-the-badge)
![Issues](https://img.shields.io/github/issues/NONAN23x/minecraft-sync?style=for-the-badge)


### Supported Platforms 🖥️

![Windows](https://img.shields.io/badge/Windows-0078D6?style=for-the-badge&logo=windows&logoColor=white)
![macOS](https://img.shields.io/badge/macOS-000000?style=for-the-badge&logo=apple&logoColor=white)
![Linux](https://img.shields.io/badge/Linux-FCC624?style=for-the-badge&logo=linux&logoColor=black)

## Purpose 🎯

Minecraft Sync was born out of frustration with the endless hassle of getting friends' Minecraft setups to match mine! 😤 If you've ever spent hours walking non-tech-savvy friends through mod installations, config tweaks, and version matching just to play together, you know the pain. This tool was originally created for my personal use and my friend group - because life's too short to troubleshoot mod conflicts when you just want to build and explore together! 🎮👥

<br>

## Distribution Model 📦

</div>

Minecraft Sync no longer needs users to install Git or Python just to get the pack.

- Users download a small Rust installer executable from GitHub Releases.
- The installer fetches `manifest.json`, downloads versioned archives from the matching release, verifies SHA-256 hashes, installs Fabric, and syncs the pack into `.minecraft`.
- The repository remains the source of truth for `mods/`, `resourcepacks/`, `shaderpacks/`, and the Fabric installer jar.

## Prerequisites ⚙️

Before running the installer, make sure the target machine already has:

1. **Java 21 or higher** - Download from [Oracle](https://www.oracle.com/java/technologies/downloads/) or [OpenJDK](https://openjdk.org/)
2. **Minecraft Launcher** - Official launcher from [minecraft.net](https://www.minecraft.net/download) or a compatible launcher such as [Prism Launcher](https://prismlauncher.org/)

> The installer creates or updates the Fabric client profile for Minecraft `26.1.2`. Launch the Fabric profile afterward, not vanilla.

## End User Install

1. Open the latest [GitHub Release](https://github.com/NONAN23x/minecraft-sync/releases/latest).
2. Download the Windows installer executable, for example `minecraft-sync.exe`.
3. Run it.

Optional flags:

```powershell
minecraft-sync.exe --minecraft-dir "D:\Games\.minecraft"
minecraft-sync.exe --skip-fabric
minecraft-sync.exe --skip-shaderpacks
```

The installer defaults to:

- `https://github.com/NONAN23x/minecraft-sync/releases/latest/download/manifest.json`
- `%APPDATA%\.minecraft` on Windows
- `~/.minecraft` on Linux
- `~/Library/Application Support/minecraft` on macOS

## Maintainer Release Flow

The release pipeline is now automated.

- Pushing a tag such as `v0.2.0` triggers GitHub Actions.
- The workflow publishes `mods.zip`, `resourcepacks.zip`, `shaderpacks.zip`, `fabric-installer-*.jar`, `manifest.json`, and platform installer binaries to the matching GitHub Release.

Create and push a release tag:

```bash
git tag v0.2.0
git push origin v0.2.0
```

Manual local publishing is still available if needed:

```bash
gh auth login
cargo build --release
python3 scripts/release.py --tag v0.2.0 --installer target/release/minecraft-sync --upload
```

The workflow and the manual flow both publish:

- `mods.zip`
- `resourcepacks.zip`
- `shaderpacks.zip`
- `fabric-installer-*.jar`
- `manifest.json`
- installer binaries

## Repository Layout

- `src/main.rs` is the Rust installer.
- `scripts/release.py` builds the release archives and uploads them with `gh`.
- `.github/workflows/release.yml` publishes release assets automatically on version tags.
- `main.py` is a deprecated legacy local-copy workflow retained only for reference.
- `release-assets/` is generated locally and ignored by git.

## Why Use Minecraft Sync? 💡

- 🐌 **Fix Vanilla Performance Issues** - Tired of 30 FPS on a decent PC? Sodium + Lithium combo delivers 3x better performance
- 🎨 **Enhanced Visuals Made Easy** - Pre-configured Iris shaders with BSL, Complementary, Bliss, and Kappa without the usual setup headache
- 📦 **Curated Mod Collection** - Hand-picked QOL mods that actually matter - no bloat, just improvements
- 🖼️ **Faithful Resource Packs** - Consistent visual overhaul that respects Minecraft's original aesthetic
- 🔧 **Skip the Config Hell** - All mods pre-tuned and compatible - no more crashes from conflicting settings
- 👥 **Everyone Stays Updated** - Your friends get the exact same setup automatically, no more "why doesn't my game look like yours?"

Stop fighting with mod incompatibilities and outdated tutorials! 🎮✨

</div>
