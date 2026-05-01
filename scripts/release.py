#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import sys
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_REPO = "NONAN23x/minecraft-sync"
PACK_FOLDERS = ("mods", "resourcepacks", "shaderpacks")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Build release archives, generate manifest.json, and optionally upload them with gh."
    )
    parser.add_argument("--tag", required=True, help="Git tag / release tag, for example v0.2.0")
    parser.add_argument("--repo", default=DEFAULT_REPO, help="GitHub repo in owner/name form")
    parser.add_argument(
        "--output-dir",
        default=str(ROOT / "release-assets"),
        help="Directory where release artifacts will be generated",
    )
    parser.add_argument(
        "--installer",
        action="append",
        default=[],
        help="Path to a built installer executable to upload alongside the archives. Repeat for multiple installers.",
    )
    parser.add_argument(
        "--fabric-jar",
        default=str(find_fabric_jar()),
        help="Path to the Fabric installer jar to include in the release manifest",
    )
    parser.add_argument(
        "--minecraft-version",
        default="26.1.2",
        help="Minecraft version encoded into the manifest",
    )
    parser.add_argument(
        "--upload",
        action="store_true",
        help="Upload artifacts to the GitHub release with gh",
    )
    return parser.parse_args()


def find_fabric_jar() -> Path:
    assets_dir = ROOT / "assets"
    jars = sorted(assets_dir.glob("fabric-installer*.jar"))
    if not jars:
        raise FileNotFoundError("No fabric-installer*.jar found in assets/")
    return jars[0]


def sha256(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def build_zip(source_dir: Path, output_path: Path) -> None:
    with ZipFile(output_path, "w", compression=ZIP_DEFLATED) as archive:
        for path in sorted(source_dir.rglob("*")):
            if path.is_file():
                archive.write(path, arcname=path.relative_to(source_dir))


def build_manifest(tag: str, repo: str, minecraft_version: str, output_dir: Path, fabric_jar: Path) -> Path:
    assets: dict[str, dict[str, object]] = {}

    for folder_name in PACK_FOLDERS:
        archive_name = f"{folder_name}.zip"
        archive_path = output_dir / archive_name
        assets[folder_name] = {
            "url": release_url(repo, tag, archive_name),
            "sha256": sha256(archive_path),
            "size": archive_path.stat().st_size,
        }

    fabric_name = fabric_jar.name
    copied_fabric_path = output_dir / fabric_name
    if copied_fabric_path.resolve() != fabric_jar.resolve():
        shutil.copy2(fabric_jar, copied_fabric_path)

    assets["fabric_installer"] = {
        "url": release_url(repo, tag, fabric_name),
        "sha256": sha256(copied_fabric_path),
        "size": copied_fabric_path.stat().st_size,
    }

    manifest = {
        "version": tag.removeprefix("v"),
        "minecraft_version": minecraft_version,
        "release_tag": tag,
        "assets": assets,
    }

    manifest_path = output_dir / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    return manifest_path


def release_url(repo: str, tag: str, file_name: str) -> str:
    return f"https://github.com/{repo}/releases/download/{tag}/{file_name}"


def ensure_release(tag: str, repo: str) -> None:
    view = subprocess.run(
        ["gh", "release", "view", tag, "--repo", repo],
        capture_output=True,
        text=True,
    )
    if view.returncode == 0:
        return

    subprocess.run(
        [
            "gh",
            "release",
            "create",
            tag,
            "--repo",
            repo,
            "--title",
            tag,
            "--notes",
            f"Release assets for {tag}",
        ],
        check=True,
    )


def upload_assets(tag: str, repo: str, files: list[Path]) -> None:
    ensure_release(tag, repo)
    subprocess.run(
        ["gh", "release", "upload", tag, "--repo", repo, "--clobber", *map(str, files)],
        check=True,
    )


def main() -> int:
    args = parse_args()
    output_dir = Path(args.output_dir).resolve() / args.tag
    output_dir.mkdir(parents=True, exist_ok=True)

    built_files: list[Path] = []

    for folder_name in PACK_FOLDERS:
        source_dir = ROOT / folder_name
        if not source_dir.is_dir():
            raise FileNotFoundError(f"Missing source directory: {source_dir}")
        archive_path = output_dir / f"{folder_name}.zip"
        build_zip(source_dir, archive_path)
        built_files.append(archive_path)

    fabric_jar = Path(args.fabric_jar).resolve()
    if not fabric_jar.is_file():
        raise FileNotFoundError(f"Missing Fabric installer jar: {fabric_jar}")

    manifest_path = build_manifest(
        tag=args.tag,
        repo=args.repo,
        minecraft_version=args.minecraft_version,
        output_dir=output_dir,
        fabric_jar=fabric_jar,
    )
    built_files.append(manifest_path)
    built_files.append(output_dir / fabric_jar.name)

    for installer in args.installer:
        installer_path = Path(installer).resolve()
        if not installer_path.is_file():
            raise FileNotFoundError(f"Installer executable not found: {installer_path}")
        installer_copy = output_dir / installer_path.name
        shutil.copy2(installer_path, installer_copy)
        built_files.append(installer_copy)

    print(f"Built release assets in {output_dir}")
    for path in built_files:
        print(f" - {path.name}")

    if args.upload:
        upload_assets(args.tag, args.repo, built_files)
        print(f"Uploaded {len(built_files)} assets to {args.repo} release {args.tag}")

    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except subprocess.CalledProcessError as error:
        print(error, file=sys.stderr)
        raise SystemExit(error.returncode)
