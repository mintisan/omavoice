#!/usr/bin/env python3
"""Collect deterministic license metadata and texts from a node_modules tree."""
import argparse
import json
import re
import shutil
import sys
from pathlib import Path

NAMES = re.compile(r"^(LICENSE.*|COPYING.*|NOTICE.*|UNLICENSE)$", re.I)


def safe(value):
    value = re.sub(r"[^A-Za-z0-9._+-]", "_", value)
    if not value or value in (".", "..") or value.startswith("."):
        raise ValueError("unsafe package path component")
    return value


def direct_packages(node_modules):
    for entry in sorted(node_modules.iterdir(), key=lambda path: path.name):
        if entry.name.startswith(".") or not entry.is_dir():
            continue
        if entry.name.startswith("@"):
            for scoped in sorted(entry.iterdir(), key=lambda path: path.name):
                if scoped.is_dir() and (scoped / "package.json").is_file():
                    yield scoped
        elif (entry / "package.json").is_file():
            yield entry


def package_directories(node_modules):
    pending = [node_modules]
    visited = set()
    while pending:
        current = pending.pop()
        resolved = current.resolve()
        if resolved in visited:
            continue
        visited.add(resolved)
        packages = list(direct_packages(current))
        for package in packages:
            yield package
        pending.extend(
            package / "node_modules"
            for package in reversed(packages)
            if (package / "node_modules").is_dir()
        )


def license_expression(value):
    if isinstance(value, str) and value:
        return value
    if isinstance(value, list):
        values = []
        for item in value:
            if isinstance(item, str):
                values.append(item)
            elif isinstance(item, dict) and isinstance(item.get("type"), str):
                values.append(item["type"])
        if values:
            return " OR ".join(values)
    raise ValueError("package has no declared license")


def repository(value):
    if isinstance(value, str):
        return value
    if isinstance(value, dict) and isinstance(value.get("url"), str):
        return value["url"]
    return ""


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("node_modules", type=Path)
    parser.add_argument("output", type=Path)
    arguments = parser.parse_args()
    node_modules = arguments.node_modules.resolve()
    if not node_modules.is_dir():
        raise ValueError("node_modules directory does not exist")
    arguments.output.mkdir(parents=True, exist_ok=True)
    rows = ["name\tversion\tlicense\trepository\tlicense_files"]
    seen = set()
    for package in package_directories(node_modules):
        metadata = json.loads((package / "package.json").read_text())
        name = metadata.get("name")
        version = metadata.get("version")
        if not isinstance(name, str) or not isinstance(version, str):
            raise ValueError(f"invalid package metadata: {package}")
        identity = (name, version)
        if identity in seen:
            continue
        seen.add(identity)
        expression = license_expression(metadata.get("license", metadata.get("licenses")))
        destination = arguments.output / safe(f"{name}-{version}")
        destination.mkdir()
        copied = []
        for source in sorted(package.iterdir(), key=lambda path: path.name):
            if not source.is_file() or not NAMES.match(source.name):
                continue
            target = destination / safe(source.name)
            shutil.copyfile(source, target)
            copied.append(source.name)
        fields = [name, version, expression, repository(metadata.get("repository")), ",".join(copied)]
        if any("\t" in field or "\n" in field for field in fields):
            raise ValueError("invalid metadata field")
        rows.append("\t".join(fields))
    if not seen:
        raise ValueError("no JavaScript packages found")
    (arguments.output / "INDEX.tsv").write_text("\n".join(rows) + "\n")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"js-licenses: {error}", file=sys.stderr)
        sys.exit(1)
