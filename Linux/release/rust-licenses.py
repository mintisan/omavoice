#!/usr/bin/env python3
"""Create a deterministic Rust dependency license directory from cargo metadata."""
import argparse, json, re, shutil, sys
from pathlib import Path

NAMES = re.compile(r"^(LICENSE.*|COPYING.*|NOTICE.*|UNLICENSE)$", re.I)
SKIPPED_DIRECTORIES = {".git", "node_modules", "target"}

def safe(value):
    value = re.sub(r"[^A-Za-z0-9._+-]", "_", value)
    if not value or value in (".", "..") or value.startswith("."):
        raise ValueError("unsafe package path component")
    return value

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("metadata", type=Path)
    ap.add_argument("output", type=Path)
    ns = ap.parse_args()
    data = json.loads(ns.metadata.read_text())
    workspace = set(data.get("workspace_members", []))
    ns.output.mkdir(parents=True, exist_ok=True)
    rows = ["name\tversion\tlicense\tsource\trepository\tlicense_files"]
    packages = sorted((p for p in data["packages"] if p["id"] not in workspace and p.get("source")),
                      key=lambda p: (p["name"], p["version"], p.get("source") or ""))
    for p in packages:
        license_id = p.get("license")
        if not license_id:
            raise RuntimeError(f"{p['name']} {p['version']} has no declared license")
        source = p.get("source") or ""
        key = safe(f"{p['name']}-{p['version']}-{source or 'unknown'}")
        dest = ns.output / key
        root = Path(p["manifest_path"]).parent.resolve()
        files = set()
        for candidate in root.rglob("*"):
            relative = candidate.relative_to(root)
            if any(part in SKIPPED_DIRECTORIES for part in relative.parts):
                continue
            if candidate.is_file() and NAMES.match(candidate.name):
                resolved = candidate.resolve()
                if resolved == root or root in resolved.parents:
                    files.add(resolved)
        lf = p.get("license_file")
        if lf:
            candidate = (root / lf).resolve()
            if candidate != root and root in candidate.parents and candidate.is_file():
                files.add(candidate)
        copied = []
        dest.mkdir()
        for src in sorted(files, key=lambda x: str(x)):
            relative = src.relative_to(root)
            components = [safe(component) for component in relative.parts]
            target = dest.joinpath(*components)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(src, target)
            copied.append(relative.as_posix())
        fields = [
            p["name"], p["version"], license_id, source,
            p.get("repository") or "", ",".join(copied),
        ]
        if any("\t" in x or "\n" in x for x in fields): raise RuntimeError("invalid metadata field")
        rows.append("\t".join(fields))
    (ns.output / "INDEX.tsv").write_text("\n".join(rows) + "\n")

if __name__ == "__main__":
    try: main()
    except Exception as e:
        print(f"rust-licenses: {e}", file=sys.stderr); sys.exit(1)
