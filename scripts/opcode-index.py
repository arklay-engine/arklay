#!/usr/bin/env python3
"""The mdbook `opcode-index` preprocessor (docs/book.toml).

- Expands `{{#opcode-index GROUP | GROUP | ...}}` in a chapter into tables of
  the opcode pages in the chapter's directory, one table per group, in the
  order given (groups not listed follow, alphabetically). Each opcode page
  starts with the header the pages share:

      # `name`

      One-sentence summary.

      | | |
      | --- | --- |
      | Opcode | `0x04` |
      | Group | Conditions |
      ...

  An optional `Status` row adds a status column.
- Fails the build when a Markdown file under the book's source is missing
  from SUMMARY.md, so a new opcode page cannot be left out of the book.

    scripts/opcode-index.py check    run only the header checks, over docs/
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DIRECTIVE = re.compile(r"\{\{#opcode-index\s*([^}]*)\}\}")
REQUIRED = ["Opcode", "Group", "Kind", "Size"]


def parse_page(path):
    """The name, summary and header rows of one opcode page."""
    lines = path.read_text().splitlines()
    errors = []
    m = re.match(r"# `([^`]+)`$", lines[0]) if lines else None
    if not m:
        return None, [f"{path}: the first line is not # `name`"]
    name = m.group(1)
    i = 1
    while i < len(lines) and not lines[i].strip():
        i += 1
    summary = []
    while i < len(lines) and lines[i].strip() and not lines[i].startswith("|"):
        summary.append(lines[i].strip())
        i += 1
    rows = {}
    for line in lines[i:]:
        if line.startswith("## "):
            break
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) == 2 and cells[0] and not set(cells[0]) <= set("- "):
            rows[cells[0]] = cells[1]
    for key in REQUIRED:
        if key not in rows:
            errors.append(f"{path}: the header has no {key} row")
    if not summary:
        errors.append(f"{path}: no summary sentence under the title")
    if path.stem != name:
        errors.append(f"{path}: the file is not named after `{name}`")
    return {"name": name, "summary": " ".join(summary), "rows": rows,
            "file": path.name}, errors


def opcode_key(page):
    m = re.search(r"0x([0-9A-Fa-f]+)", page["rows"].get("Opcode", ""))
    return (int(m.group(1), 16) if m else 1 << 30, page["name"])


def render_index(directory, order):
    pages, errors = [], []
    for path in sorted(directory.glob("*.md")):
        if path.name == "index.md":
            continue
        page, errs = parse_page(path)
        errors += errs
        if page:
            pages.append(page)
    groups = {}
    for page in pages:
        groups.setdefault(page["rows"].get("Group", "Other"), []).append(page)
    names = [g for g in order if g in groups]
    names += sorted(g for g in groups if g not in order)
    for g in order:
        if g not in groups:
            errors.append(f"{directory}: no opcode page is in group {g!r}")
    status = any("Status" in p["rows"] for p in pages)
    out = []
    for g in names:
        out.append(f"## {g}\n")
        head = "| Opcode | Instruction | Summary |" + (" Status |" if status else "")
        out.append(head)
        out.append("| --- | --- | --- |" + (" --- |" if status else ""))
        for p in sorted(groups[g], key=opcode_key):
            opcode = p["rows"].get("Opcode", "").replace("|", "\\|")
            row = f"| {opcode} | [`{p['name']}`]({p['file']}) | {p['summary'].replace('|', chr(92) + '|')} |"
            if status:
                row += f" {p['rows'].get('Status', '')} |"
            out.append(row)
        out.append("")
    return "\n".join(out), errors


def chapters_of(book):
    found = []

    def walk(items):
        for entry in items:
            ch = entry.get("Chapter") if isinstance(entry, dict) else None
            if ch:
                found.append(ch)
                walk(ch.get("sub_items", []))

    walk(book.get("items", book.get("sections", [])))
    return found


def preprocess():
    context, book = json.load(sys.stdin)
    src = (Path(context["root"]) / context["config"]["book"].get("src", "src")).resolve()
    chapters = chapters_of(book)
    errors = []
    listed = {ch["path"] for ch in chapters if ch.get("path")}
    for md in sorted(src.rglob("*.md")):
        rel = md.relative_to(src).as_posix()
        if rel != "SUMMARY.md" and rel not in listed:
            errors.append(f"{rel} is not in SUMMARY.md")
    for ch in chapters:
        if not ch.get("path"):
            continue
        m = DIRECTIVE.search(ch["content"])
        if not m:
            continue
        order = [g.strip() for g in m.group(1).split("|") if g.strip()]
        table, errs = render_index((src / ch["path"]).parent, order)
        errors += errs
        ch["content"] = DIRECTIVE.sub(lambda _m: table, ch["content"])
    for e in errors:
        print(f"opcode-index: {e}", file=sys.stderr)
    if errors:
        return 1
    json.dump(book, sys.stdout)
    return 0


def check():
    errors = []
    for directory in sorted((ROOT / "docs").rglob("opcodes")):
        for path in sorted(directory.glob("*.md")):
            if path.name != "index.md":
                errors += parse_page(path)[1]
    for e in errors:
        print(e, file=sys.stderr)
    return 1 if errors else 0


def main(argv):
    if len(argv) >= 2 and argv[1] == "supports":
        return 0 if argv[2] == "html" else 1
    if len(argv) >= 2 and argv[1] == "check":
        return check()
    if len(argv) == 1:
        return preprocess()
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
