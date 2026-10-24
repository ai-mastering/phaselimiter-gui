#!/usr/bin/env python3
"""Collect dependency metadata and packaged license/notice texts for a bundle."""
import json
import pathlib
import subprocess
import sys

target, destination = sys.argv[1:]
metadata = json.loads(subprocess.check_output([
    "cargo", "metadata", "--locked", "--format-version=1", "--filter-platform", target,
], text=True))
nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
pending = [metadata["resolve"]["root"]]
included = set()
while pending:
    identity = pending.pop()
    if identity in included:
        continue
    included.add(identity)
    for dependency in nodes[identity]["deps"]:
        if any(kind["kind"] != "dev" for kind in dependency["dep_kinds"]):
            pending.append(dependency["pkg"])

parts = ["Third-party dependencies and license notices\n"]
for package in sorted(metadata["packages"], key=lambda package: (package["name"], package["version"])):
    if package["id"] not in included or package["id"] == metadata["resolve"]["root"]:
        continue
    parts.append(f"\n{'=' * 72}\n{package['name']} {package['version']}\n"
                 f"License: {package.get('license') or 'See license text'}\n"
                 f"Source: {package.get('repository') or package.get('source') or ''}\n")
    directory = pathlib.Path(package["manifest_path"]).parent
    texts = set()
    for path in directory.rglob("*"):
        if not path.is_file():
            continue
        relative = path.relative_to(directory)
        name = path.name.upper()
        if not (name.startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "OFL"))
                or (len(relative.parts) <= 3 and "licenses" in relative.parts)):
            continue
        try:
            contents = path.read_text(encoding="utf-8")
        except (UnicodeError, OSError):
            continue
        if contents in texts:
            continue
        texts.add(contents)
        parts.append(f"\n--- {relative} ---\n{contents}\n")
    if not texts:
        parts.append(f"License text is in the upstream source distribution: https://crates.io/crates/{package['name']}/{package['version']}\n")

pathlib.Path(destination).write_text("".join(parts), encoding="utf-8")
