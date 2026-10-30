#!/usr/bin/env python3
"""Build matching host/helper binaries without fetching a separate engine checkout."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("surface", choices=["cli", "gpui", "tauri"])
parser.add_argument("--out", type=Path, required=True)
parser.add_argument("--target", default=os.environ.get("TAURI_ENV_TARGET_TRIPLE"))
parser.add_argument("--profile", choices=["dev", "release"], default="dev")
parser.add_argument("--target-dir", type=Path, default=ROOT / "target/local-host")
parser.add_argument("--helper-only", action="store_true")
parser.add_argument("--offline", action="store_true", help="use only dependencies already cached by Cargo")
parser.add_argument("--native-layout", action="store_true", help="reuse native Cargo output layout for local development")
args = parser.parse_args()
host = next(line.split(": ", 1)[1] for line in subprocess.check_output(["rustc", "-vV"], text=True).splitlines() if line.startswith("host: "))
if not args.target:
    args.target = host
if not (args.target.endswith("apple-darwin") or args.target.endswith("unknown-linux-gnu")):
    parser.error("isolated script execution requires a supported Unix build target")
if args.surface == "gpui" and not args.target.endswith("apple-darwin"):
    parser.error("GPUI app staging currently supports macOS only")
if args.native_layout and args.target != host:
    parser.error("native layout requires the host target")
args.out = args.out.resolve()
args.target_dir = args.target_dir.resolve()
helper_manifest = ROOT / "ui/tui/Cargo.toml"
manifest, binary = {"cli": (helper_manifest, "tk"), "gpui": (ROOT / "ui/gui/Cargo.toml", "telekinesis-companion"), "tauri": (ROOT / "apps/desktop/src-tauri/Cargo.toml", "tk-desktop")}[args.surface]

def metadata(path):
    value = json.loads(subprocess.check_output(["cargo", "metadata", *(["--offline"] if args.offline else []), "--locked", "--filter-platform", args.target, "--format-version", "1", "--manifest-path", str(path)], cwd=ROOT, text=True))
    engine = next(package for package in value["packages"] if package["name"] == "rx4")
    return engine

engine = metadata(helper_manifest)
if metadata(manifest)["id"] != engine["id"]:
    raise SystemExit("host/helper resolve different engine sources; align locks before staging")

def engine_digest():
    directory = Path(engine["manifest_path"]).parent
    files = sorted((directory / "src").rglob("*.rs")) + [directory / "Cargo.toml", directory / "Cargo.lock", directory / "build.rs"]
    digest = hashlib.sha256()
    for file in files:
        if file.is_file():
            digest.update(str(file.relative_to(directory)).encode())
            digest.update(file.read_bytes())
    return digest.hexdigest()

def host_digest():
    digest = hashlib.sha256()
    for relative in ["ui/tui", "ui/gui", "ui/router", "apps/desktop/src-tauri"]:
        directory = ROOT / relative
        files = sorted((directory / "src").rglob("*.rs")) + sorted(directory.glob("*.crepus"))
        files += [directory / "Cargo.toml", directory / "Cargo.lock"]
        for file in files:
            if file.is_file():
                digest.update(str(file.relative_to(ROOT)).encode())
                digest.update(file.read_bytes())
    return digest.hexdigest()

source_digest = engine_digest()
host_source_digest = host_digest()

def build(path, name):
    command = ["cargo", "build", *(["--offline"] if args.offline else []), "--locked", "--manifest-path", str(path), "--bin", name, "--target-dir", str(args.target_dir)]
    if not args.native_layout:
        command += ["--target", args.target]
    if args.profile == "release":
        command.append("--release")
    subprocess.run(command, cwd=ROOT, check=True)
    return args.target_dir / ("" if args.native_layout else args.target) / ("release" if args.profile == "release" else "debug") / name

helper = build(helper_manifest, "rx4-script-worker")
if args.surface == "tauri":
    sidecar = manifest.parent / "binaries" / f"rx4-script-worker-{args.target}"
    sidecar.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(helper, sidecar)
if args.surface == "gpui":
    app = args.out / "Telekinesis.app"
    destination = app / "Contents/MacOS"
    destination.mkdir(parents=True, exist_ok=True)
    with (app / "Contents/Info.plist").open("wb") as stream:
        plistlib.dump({"CFBundleExecutable": binary, "CFBundleIdentifier": "hk.tsc.telekinesis.local", "CFBundleName": "Telekinesis", "CFBundlePackageType": "APPL", "LSEnvironment": {"TK_LOCAL_RUNTIME_ROOT": str(args.out / "state")}}, stream)
else:
    destination = args.out / "bin"
    destination.mkdir(parents=True, exist_ok=True)
shutil.copy2(helper, destination / helper.name)
if not args.helper_only:
    host = build(manifest, binary)
    shutil.copy2(host, destination / host.name)
if engine_digest() != source_digest or host_digest() != host_source_digest:
    raise SystemExit("host/engine source changed during staging; discard these artifacts and rebuild")
args.out.mkdir(parents=True, exist_ok=True)
(args.out / "build-source.json").write_text(json.dumps({"surface": args.surface, "target": args.target, "profile": args.profile, "engine": {key: engine[key] for key in ("id", "version", "source", "manifest_path")}, "helperSha256": hashlib.sha256(helper.read_bytes()).hexdigest(), "engineSourceSha256": source_digest, "hostSourceSha256": host_source_digest, "signed": False}, indent=2) + "\n")
print(f"Staged {args.surface} at {args.out}; distribution signing is a separate step.")
