# AGENTS.md

## Purpose

This file persists project context for future coding agents working in this repository.

## Project Summary

- Project: `minecraft-sync`
- Current primary product: a minimal Rust installer for a curated Minecraft modpack
- Current version in `Cargo.toml`: `0.2.0`
- Latest pushed release tag: `v0.2.1`
- Canonical distribution model: GitHub Releases

Users are expected to download a prebuilt installer binary from GitHub Releases, run it locally, and let it fetch versioned release assets from the matching release.

## Source Of Truth

- `src/main.rs`
  The primary installer implementation.
- `scripts/release.py`
  Builds release archives, generates `manifest.json`, and can upload release assets with `gh`.
- `.github/workflows/release.yml`
  Automated release publishing on version tags.
- `mods/`, `resourcepacks/`, `shaderpacks/`
  Pack content that becomes release archives.
- `assets/fabric-installer-1.1.1.jar`
  Fabric installer jar included in release assets and referenced by the manifest.

## Runtime Model

The Rust installer:

- fetches `manifest.json` from GitHub Releases by default
- downloads release assets from GitHub
- verifies file size and SHA-256
- installs Fabric by invoking `java -jar <fabric-installer>.jar`
- syncs `mods`, `resourcepacks`, and `shaderpacks` into the user Minecraft directory
- creates backups before replacing pack folders
- rolls back folder changes on extraction failure

Default manifest URL in code:

- `https://github.com/NONAN23x/minecraft-sync/releases/latest/download/manifest.json`

## Release Model

Current intended release path is automated.

- Pushing a tag matching `v*` triggers `.github/workflows/release.yml`.
- The workflow publishes:
  - `mods.zip`
  - `resourcepacks.zip`
  - `shaderpacks.zip`
  - `fabric-installer-1.1.1.jar`
  - `manifest.json`
  - installer binaries for Windows, macOS, and Linux

Manual fallback remains available:

```bash
cargo build --release
python3 scripts/release.py --tag v0.2.0 --upload --installer target/release/minecraft-sync
```

Notes:

- `scripts/release.py` supports repeating `--installer` multiple times.
- Local `gh` publishing depends on valid local auth.
- Later in this session, `gh auth status` showed `NONAN23x` authenticated with `repo` and `workflow` scopes.
- GitHub Actions was triggered successfully by pushing tag `v0.2.1`.

## Deprecated Paths

- `main.py` is deprecated and retained only as a legacy reference.
- `ROADMAP.md` no longer defines the active implementation plan. It has been repurposed as migration status.

Do not treat the Python sync workflow as the supported user path unless explicitly asked to revive or maintain it.

## Current Migration Status

The Rust migration is complete and has been committed and pushed.

- Rust is the primary installer path.
- GitHub Actions is the primary release publication path.
- Migration commit: `3d2c898` (`Migrate installer and release flow to Rust`)
- Tag `v0.2.1` was pushed to trigger the new release workflow.

Remaining work is operational rather than architectural:

- verify the final asset list on the `v0.2.1` GitHub Release
- smoke-test one released installer on a clean Minecraft directory
- optionally retire or supersede `v0.2.0` if the older release shape should no longer be used

## Repo Conventions For Future Agents

- Prefer the Rust installer path over extending the Python legacy workflow.
- Keep release assets GitHub-compatible; `manifest.json` URLs must match release asset names exactly.
- If changing release asset naming, update both:
  - `scripts/release.py`
  - `src/main.rs` behavior and any docs that reference release downloads
- Avoid introducing a second distribution path unless explicitly requested.
- Preserve the current expectation that Java must already be installed on the target machine.

## Validation Checklist

When modifying the Rust installer or release flow, validate at least:

```bash
cargo check
python3 -m py_compile scripts/release.py main.py
python3 scripts/release.py --help
```

If changing packaging or manifest generation, also validate:

```bash
python3 scripts/release.py --tag v0.2.0
```

If publishing a new release:

```bash
git push origin master
git tag v0.2.1
git push origin v0.2.1
```

## Git Notes

- At the time of this update, `git status --short` was clean.
- Do not revert unrelated user changes.
- Generated local paths that should remain ignored:
  - `target/`
  - `release-assets/`
  - `__pycache__/`
  - `.codex`

## If Asked To Publish

Check both:

1. Local `gh` authentication with `gh auth status`
2. Whether tag-driven GitHub Actions publication is sufficient

Current known state:

- local `gh` auth worked for push and release inspection in this session
- `v0.2.1` release exists on GitHub
- archive assets were confirmed on the release
- if platform installer uploads matter, explicitly verify the final release asset list rather than assuming the matrix completed
