#!/usr/bin/env python3
"""Checks the relative links in the documentation: the file exists and, for a .md, the
anchor matches a heading (GitHub's rule: lowercase, backticks and punctuation dropped except
"-" and "_", spaces become "-"). Splitting or renaming a document breaks links silently;
run this after any such change.

Usage: tools/check-links.py [FILE.md ...]   (default: README*.md, docs/*.md, reference/*.md)
Exits with 1 if a link is broken. docs/kit/ (templates) is skipped.
"""
import os
import pathlib
import re
import sys

root = pathlib.Path(__file__).resolve().parent.parent


def slug(title):
    title = title.strip().lower().replace("`", "")
    return "".join(c for c in title if c.isalnum() or c in " -_").replace(" ", "-")


def anchors(path, cache={}):
    if path not in cache:
        cache[path] = {slug(h) for h in re.findall(r"^#+ (.*)$", path.read_text(), re.M)}
    return cache[path]


files = [pathlib.Path(f).resolve() for f in sys.argv[1:]] or sorted(
    p for pattern in ("README*.md", "docs/*.md", "reference/*.md") for p in root.glob(pattern)
)
bad = total = 0
for f in files:
    text = re.sub(r"```.*?```", "", f.read_text(), flags=re.S)  # links inside code don't count
    text = re.sub(r"`[^`\n]*`", "", text)
    text = re.sub(r"<!--.*?-->", "", text, flags=re.S)  # nor inside TODO comments
    for link in re.findall(r"\]\(([^)\s]+)\)", text):
        if re.match(r"[a-z]+:", link):
            continue
        total += 1
        path, _, anchor = link.partition("#")
        target = (f.parent / path).resolve() if path else f
        if not target.exists():
            print(f"{os.path.relpath(f)}: missing file: {link}")
            bad += 1
        elif anchor and target.suffix == ".md" and anchor not in anchors(target):
            print(f"{os.path.relpath(f)}: missing anchor: {link}")
            bad += 1
print(f"{total} links, {bad} broken")
sys.exit(1 if bad else 0)
