#!/usr/bin/env python3
"""Package the local alpha app and matching working-tree source with checksums."""
import argparse
import hashlib
import json
from pathlib import Path
import plistlib
import subprocess
import tarfile


ROOT = Path(__file__).resolve().parents[1]


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, default=ROOT / "src-tauri/target/release/bundle/macos/OpenChippy.app")
    parser.add_argument("--output", type=Path, required=True, help="new output directory outside the source tree")
    args = parser.parse_args()
    app = args.app.resolve()
    output = args.output.resolve()
    if output == ROOT or ROOT in output.parents:
        parser.error("output must be outside the source tree")
    version = json.loads((ROOT / "package.json").read_text())["version"]
    with (app / "Contents/Info.plist").open("rb") as stream:
        metadata = plistlib.load(stream)
    if metadata.get("CFBundleShortVersionString") != version:
        parser.error("app version does not match package.json; rebuild the alpha bundle")
    executable = app / "Contents/MacOS" / metadata["CFBundleExecutable"]
    architecture = subprocess.check_output(["/usr/bin/file", str(executable)], text=True).strip()
    if "arm64" not in architecture:
        parser.error("this packager expects the Apple Silicon build")
    resources = app / "Contents/Resources"
    resource_sources = {
        "LICENSE": "LICENSE",
        "ALPHA-RELEASE-NOTES.md": "docs/releases/alpha-v1.md",
        "examples/qualification/inverter.chippy": "docs/examples/qualification/inverter.chippy",
        "examples/qualification/nand2.chippy": "docs/examples/qualification/nand2.chippy",
        "examples/rtl/alpha-inverter.v": "docs/examples/rtl/alpha-inverter.v",
    }
    for name, source_name in resource_sources.items():
        if not (resources / name).is_file():
            parser.error(f"missing app resource: {name}")
        if sha256(resources / name) != sha256(ROOT / source_name):
            parser.error(f"stale app resource: {name}; rebuild the alpha bundle")
    output.mkdir(parents=True, exist_ok=False)
    archive = output / f"OpenChippy-{version}-macos-arm64.zip"
    subprocess.run(["/usr/bin/ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", str(app), str(archive)], check=True)
    source = output / f"OpenChippy-{version}-source.tar.gz"
    # Include uncommitted alpha work and new source files. Ignored dependencies,
    # build outputs and .git are excluded by the repository's file inventory.
    files = subprocess.check_output([
        "git", "ls-files", "-z", "--cached", "--others", "--exclude-standard",
    ], cwd=ROOT).decode().split("\0")
    with tarfile.open(source, "w:gz") as bundle:
        for name in sorted(set(filter(None, files))):
            path = ROOT / name
            if path.exists() or path.is_symlink():
                bundle.add(path, arcname=f"OpenChippy-{version}/{name}", recursive=False)
    notes = output / "ALPHA-RELEASE-NOTES.md"
    notes.write_bytes((ROOT / "docs/releases/alpha-v1.md").read_bytes())
    artifacts = [archive, source, notes]
    manifest = {
        "version": version,
        "sourceCommit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "workingTreeDirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT)),
        "platform": "macos-arm64", "signed": False, "notarized": False,
        "bundleVersion": metadata.get("CFBundleVersion"),
        "bundleShortVersion": metadata.get("CFBundleShortVersionString"),
        "executableSha256": sha256(executable),
        "artifacts": {path.name: sha256(path) for path in artifacts},
    }
    manifest_path = output / "release-manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
    (output / "SHA256SUMS").write_text("".join(
        f"{sha256(path)}  {path.name}\n" for path in artifacts + [manifest_path]
    ))
    print(f"Prepared {version}: {output}")


if __name__ == "__main__":
    main()
