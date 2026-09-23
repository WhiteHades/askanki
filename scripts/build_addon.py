from __future__ import annotations

import argparse
import json
import platform
import stat
import subprocess
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def default_target() -> str:
    machine = platform.machine().lower()
    architecture = {
        "x86_64": "x86_64",
        "amd64": "x86_64",
        "aarch64": "aarch64",
        "arm64": "aarch64",
    }.get(machine, machine)
    system = platform.system().lower()
    if system == "windows":
        return f"{architecture}-pc-windows-msvc"
    if system == "darwin":
        return f"{architecture}-apple-darwin"
    return f"{architecture}-unknown-linux-gnu"


def run(command: list[str], cwd: Path) -> None:
    subprocess.run(command, cwd=cwd, check=True)


def add_file(archive: zipfile.ZipFile, source: Path, archive_name: str, executable: bool = False) -> None:
    info = zipfile.ZipInfo(archive_name)
    info.date_time = (1980, 1, 1, 0, 0, 0)
    info.compress_type = zipfile.ZIP_DEFLATED
    mode = stat.S_IFREG | (0o755 if executable else 0o644)
    info.external_attr = mode << 16
    archive.writestr(info, source.read_bytes())


def build(target: str, output: Path) -> Path:
    run(["npm", "run", "build"], ROOT / "frontend")
    run(["cargo", "build", "--release", "--locked", "--target", target], ROOT / "native")
    binary_name = "askanki-core.exe" if target.endswith("-windows-msvc") else "askanki-core"
    binary = ROOT / "native" / "target" / target / "release" / binary_name
    if not binary.is_file():
        raise FileNotFoundError(f"native binary was not produced: {binary}")
    frontend_dist = ROOT / "frontend" / "dist"
    if not (frontend_dist / "assets" / "index.js").is_file() or not (frontend_dist / "assets" / "index.css").is_file():
        raise FileNotFoundError("frontend production assets are incomplete")
    output.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for filename in [
            "__init__.py",
            "askanki_bridge.py",
            "config.json",
            "config.md",
            "LICENSE",
            "README.md",
            "THIRD_PARTY_NOTICES.md",
            "manifest.json",
        ]:
            add_file(archive, ROOT / filename, filename)
        for path in sorted(frontend_dist.rglob("*")):
            if path.is_file():
                add_file(archive, path, (Path("frontend/dist") / path.relative_to(frontend_dist)).as_posix())
        add_file(archive, binary, (Path("native") / target / binary_name).as_posix(), executable=True)
    return output


def main() -> None:
    parser = argparse.ArgumentParser(description="Build the AskAnki Anki add-on archive")
    parser.add_argument("--target", default=default_target())
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    version = json.loads((ROOT / "manifest.json").read_text())["version"]
    output = args.output or ROOT / "dist" / f"askanki-{version}-{args.target.replace('-', '_')}.ankiaddon"
    if not output.is_absolute():
        output = Path.cwd() / output
    print(build(args.target, output))


if __name__ == "__main__":
    main()
