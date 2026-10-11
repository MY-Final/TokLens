"""Validate versions and assemble a complete signed release without network access."""
import argparse
import datetime
import json
import re
import shutil
import tomllib
from pathlib import Path

REPOSITORY = "MY-Final/TokLens"
TARGETS = {
    "x86_64-pc-windows-msvc": "windows-x86_64-nsis",
    "x86_64-apple-darwin": "darwin-x86_64",
    "aarch64-apple-darwin": "darwin-aarch64",
}


def validate(root: Path, tag: str) -> str:
    if not re.fullmatch(r"v\d+\.\d+\.\d+", tag):
        raise ValueError("Only stable vX.Y.Z release tags are accepted")
    version = tag[1:]
    versions = [
        json.loads((root / "package.json").read_text(encoding="utf-8"))["version"],
        json.loads((root / "src-tauri/tauri.conf.json").read_text(encoding="utf-8"))["version"],
        tomllib.loads((root / "src-tauri/Cargo.toml").read_text(encoding="utf-8"))["package"]["version"],
    ]
    if any(item != version for item in versions):
        raise ValueError("Tag, package.json, Cargo.toml and Tauri version must match")
    return version


def only(directory: Path, pattern: str) -> Path:
    matches = sorted(directory.glob(pattern))
    if len(matches) != 1:
        raise ValueError(f"Expected exactly one {pattern} in {directory}")
    return matches[0]


def collect(root: Path, target: str, version: str, output: Path) -> None:
    platform = TARGETS[target]
    bundles = root / "src-tauri/target" / target / "release/bundle"
    output.mkdir(parents=True, exist_ok=False)
    prefix = f"TokLens_{version}_{platform}"
    if platform.startswith("windows"):
        package = only(bundles / "nsis", "*-setup.exe")
        name = prefix + "-setup.exe"
    else:
        package = only(bundles / "macos", "*.app.tar.gz")
        name = prefix + ".app.tar.gz"
        disk = only(bundles / "dmg", "*.dmg")
        shutil.copy2(disk, output / (prefix + ".dmg"))
    signature_path = Path(str(package) + ".sig")
    signature = signature_path.read_text(encoding="utf-8").strip()
    if not signature:
        raise ValueError("Update signature is empty")
    shutil.copy2(package, output / name)
    shutil.copy2(signature_path, output / (name + ".sig"))
    fragment = {"version": version, "platform": platform, "name": name, "signature": signature}
    (output / "fragment.json").write_text(json.dumps(fragment), encoding="utf-8")


def assemble(source: Path, output: Path, version: str, notes: str, date: str) -> None:
    platforms = {}
    fragments = sorted(source.rglob("fragment.json"))
    if len(fragments) != len(TARGETS):
        raise ValueError("All three platform builds must finish before release assembly")
    output.mkdir(parents=True, exist_ok=False)
    for file in fragments:
        fragment = json.loads(file.read_text(encoding="utf-8"))
        platform, name = fragment["platform"], fragment["name"]
        if platform not in TARGETS.values() or platform in platforms or fragment["version"] != version:
            raise ValueError("Invalid, duplicate or inconsistent platform fragment")
        if Path(name).name != name or not fragment["signature"].strip():
            raise ValueError("Invalid asset name or empty signature")
        if (file.parent / (name + ".sig")).read_text(encoding="utf-8").strip() != fragment["signature"]:
            raise ValueError("Signature metadata does not match the artifact")
        if not (file.parent / name).is_file():
            raise ValueError("Missing updater package")
        platforms[platform] = {
            "url": f"https://github.com/{REPOSITORY}/releases/download/v{version}/{name}",
            "signature": fragment["signature"],
        }
        for asset in file.parent.iterdir():
            if asset.name != "fragment.json" and asset.is_file():
                if (output / asset.name).exists():
                    raise ValueError("Duplicate release asset name")
                shutil.copy2(asset, output / asset.name)
    if set(platforms) != set(TARGETS.values()):
        raise ValueError("Incomplete platform manifest")
    manifest = {"version": version, "notes": notes, "pub_date": date, "platforms": platforms}
    (output / "latest.json").write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=["validate", "collect", "assemble"])
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--tag", required=True)
    parser.add_argument("--target", choices=TARGETS)
    parser.add_argument("--input", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    version = validate(args.root, args.tag)
    if args.action == "collect":
        if not args.target or not args.output:
            parser.error("collect requires --target and --output")
        collect(args.root, args.target, version, args.output)
    elif args.action == "assemble":
        if not args.input or not args.output:
            parser.error("assemble requires --input and --output")
        notes = (args.root / "RELEASE_NOTES.md").read_text(encoding="utf-8")
        date = datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z")
        assemble(args.input, args.output, version, notes, date)
    print(f"Validated {args.action}: {version}")


if __name__ == "__main__":
    main()
