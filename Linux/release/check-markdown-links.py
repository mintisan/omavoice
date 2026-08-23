#!/usr/bin/env python3
"""Reject broken or escaping relative links in a Markdown tree."""

import argparse
import re
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

LINK = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("root", type=Path)
    arguments = parser.parse_args()
    root = arguments.root.resolve()
    if not root.is_dir():
        raise ValueError("Markdown root does not exist")

    failures = []
    for document in sorted(root.rglob("*.md")):
        for line_number, line in enumerate(document.read_text().splitlines(), 1):
            for match in LINK.finditer(line):
                raw = match.group(1).strip()
                if raw.startswith("<") and raw.endswith(">"):
                    raw = raw[1:-1]
                raw = raw.split(maxsplit=1)[0]
                parsed = urlsplit(raw)
                if parsed.scheme or parsed.netloc or not parsed.path:
                    continue
                relative = Path(unquote(parsed.path))
                target = (document.parent / relative).resolve()
                if target != root and root not in target.parents:
                    failures.append(f"{document.relative_to(root)}:{line_number}: link escapes root: {raw}")
                elif not target.exists():
                    failures.append(f"{document.relative_to(root)}:{line_number}: missing link: {raw}")
    if failures:
        raise RuntimeError("\n".join(failures))


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print(f"check-markdown-links: {error}", file=sys.stderr)
        sys.exit(1)
