#!/usr/bin/env python3
"""Turn the LuaLS definition files in lua-types/ into the Lua API pages.

The definition files are the single source of the Lua API documentation:
the binary ships them (`arklay mod lua-types`) for editors to lint and
complete scripts with, and this script renders them for the book.

    scripts/lua-api-doc.py check          every method the engine registers is
                                          documented, and nothing else is
    scripts/lua-api-doc.py render FILE    print one file's page as Markdown
    (mdbook)                              as the `lua-api` preprocessor
                                          (docs/book.toml) it expands each
                                          `{{#lua-api FILE}}` in a chapter,
                                          FILE naming a file in lua-types/

Format, a subset of LuaLS annotations:

- `---` lines are documentation (Markdown); plain `--` comments are not.
- `-- # Title` starts a group of definitions on the page.
- `-- source: src/a.rs src/b.rs#Type` (before a `---@class`) names the Rust
  files that register the class's methods, for `check`; `#Type` narrows a
  file to its `impl UserData for Type` block.
- `---@class Name` / `---@field name type desc` before `local Name = {}`.
- `---@alias Name type` documents a type name.
- `---@param name[?] type desc` and `---@return type [name] [desc]` before
  `function Class:method(...) end`, `function Class.fn(...) end` or
  `function global(...) end`.
- A `---` block followed by anything else (a blank line, a `-- #` line) is
  prose; the file's first one is the page introduction.
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LUA_DIR = ROOT / "lua-types"
DIRECTIVE = re.compile(r"\{\{#lua-api\s+([^}\s]+)\s*\}\}")
FUNC = re.compile(r"^function\s+([A-Za-z_][\w]*)(?:([:.])([A-Za-z_]\w*))?\s*\(([^)]*)\)")
LOCAL_TABLE = re.compile(r"^local\s+([A-Za-z_]\w*)\s*=")
REGISTER = re.compile(r'add_(?:async_)?(?:method|function)(?:_mut)?\(\s*"([^"]+)"')


class Item:
    def __init__(self, kind, **kw):
        self.kind = kind  # prose | group | function | class | alias
        self.__dict__.update(kw)


def split_tags(doc_lines):
    """Split a doc block into description text and @tag lines."""
    text, tags = [], []
    for line in doc_lines:
        if line.startswith("@"):
            tags.append(line)
        elif tags and not line.strip() == "" and not tags[-1].startswith("@class"):
            # A continuation of the previous tag's description.
            tags[-1] += " " + line.strip()
        else:
            text.append(line)
    return "\n".join(text).strip(), tags


def parse_type_and_rest(s):
    """Split a type expression from the description after it."""
    s = s.strip()
    depth, i = 0, 0
    while i < len(s):
        c = s[i]
        if c in "{([<":
            depth += 1
        elif c in "})]>":
            depth -= 1
        elif c == " " and depth == 0:
            # `a | b` style unions keep going.
            rest = s[i:].lstrip()
            if rest.startswith("|"):
                i = len(s) - len(rest) + 1
                while i < len(s) and s[i] == " ":
                    i += 1
                continue
            if s[i - 1] == "|":
                i += 1
                continue
            return s[:i], s[i:].strip()
        i += 1
    return s, ""


def parse_file(path):
    items = []
    doc = []
    source = []
    lines = path.read_text().splitlines()
    for raw in lines + [""]:
        line = raw.rstrip()
        if line.startswith("---") and not line.startswith("---@meta"):
            doc.append(line[3:])
            continue
        # `--- text` and `---text` are both doc lines; strip the space only
        # when the whole block uses it, so code keeps its indentation.
        if all(d.startswith(" ") for d in doc if d.strip()):
            doc = [d[1:] for d in doc]
        text, tags = split_tags(doc)
        doc = []
        m_func = FUNC.match(line)
        m_local = LOCAL_TABLE.match(line)
        if m_func and (text or tags):
            owner, sep, name, args = m_func.groups()
            if not name:
                owner, sep, name = None, None, owner
            params, returns = [], []
            for tag in tags:
                if tag.startswith("@param "):
                    pname, _, rest = tag[7:].strip().partition(" ")
                    ptype, pdesc = parse_type_and_rest(rest)
                    optional = pname.endswith("?") or ptype.endswith("?")
                    params.append((pname.rstrip("?"), ptype, pdesc, optional))
                elif tag.startswith("@return "):
                    rtype, rest = parse_type_and_rest(tag[8:])
                    rname, rdesc = "", rest
                    m = re.match(r"([A-Za-z_]\w*)(\s+(.*))?$", rest)
                    if m and (m.group(3) or not rest.endswith(".")):
                        rname, rdesc = m.group(1), (m.group(3) or "")
                    returns.append((rtype, rname, rdesc))
            items.append(Item("function", owner=owner, sep=sep, name=name,
                              args=[a.strip() for a in args.split(",") if a.strip()],
                              text=text, params=params, returns=returns))
        elif tags and any(t.startswith("@class ") for t in tags):
            cname = next(t for t in tags if t.startswith("@class "))[7:].split()[0]
            cname = cname.split(":")[0]
            fields = []
            for tag in tags:
                if tag.startswith("@field "):
                    fname, _, rest = tag[7:].strip().partition(" ")
                    ftype, fdesc = parse_type_and_rest(rest)
                    fields.append((fname.rstrip("?"), ftype, fdesc, fname.endswith("?")))
            items.append(Item("class", name=cname, text=text, fields=fields,
                              source=source))
            source = []
        elif tags and any(t.startswith("@alias ") for t in tags):
            tag = next(t for t in tags if t.startswith("@alias "))
            aname, _, rest = tag[7:].strip().partition(" ")
            items.append(Item("alias", name=aname, type=rest.strip(), text=text))
        elif text:
            items.append(Item("prose", text=text))
        if line.startswith("-- # "):
            items.append(Item("group", title=line[5:].strip()))
        elif line.startswith("-- source:"):
            source = line[len("-- source:"):].split()
        _ = m_local
    return items


# ---------------------------------------------------------------- rendering

def anchor(item):
    if item.owner:
        return f"{item.owner}.{item.name}"
    return item.name


def signature(item):
    args = []
    optional = {p[0] for p in item.params if p[3]}
    for a in item.args:
        args.append(a + ("?" if a in optional else ""))
    joined = ", ".join(args)
    if item.owner:
        return f"{item.owner}{item.sep}{item.name}({joined})"
    return f"{item.name}({joined})"


def cell(s):
    return s.replace("|", "\\|").replace("\n", " ")


def first_sentence(text):
    para = text.split("\n\n")[0].replace("\n", " ")
    m = re.match(r"(.+?\.)(\s|$)", para)
    return m.group(1) if m else para


class Renderer:
    def __init__(self, index):
        # index: type name -> (page path relative to src, anchor)
        self.index = index

    def type_md(self, t, page):
        """A type as code, with known class and alias names linked."""
        def link(name):
            if name in self.index:
                target_page, target = self.index[name]
                href = "" if target_page == page else relpath(target_page, page)
                return f"[`{name}`]({href}#{target})"
            return None

        bare = t.rstrip("?")
        linked = link(bare.rstrip("[]"))
        if linked and re.fullmatch(r"[A-Za-z_]\w*(\[\])?", bare):
            return linked + ("`[]`" if bare.endswith("[]") else "") + ("`?`" if t.endswith("?") else "")
        return f"`{t}`"

    def render(self, items, page):
        out = []
        in_class = False
        # Whether the last heading was a group outside any class, so an alias
        # after it belongs to that group rather than standing on its own.
        top_group = False
        intro_done = False
        for item in items:
            if item.kind == "prose":
                out.append(item.text + "\n")
                intro_done = True
            elif item.kind == "group":
                out.append(("### " if in_class else "## ") + item.title + "\n")
                top_group = not in_class
            elif item.kind == "alias":
                level = "###" if top_group else "##"
                out.append(f"{level} `{item.name}` {{#{item.name}}}\n")
                out.append(f"Alias of `{item.type}`.\n")
                if item.text:
                    out.append(item.text + "\n")
            elif item.kind == "class":
                in_class = True
                top_group = False
                out.append(f"## `{item.name}` {{#{item.name}}}\n")
                if item.text:
                    out.append(item.text + "\n")
                if item.fields:
                    out.append("| Field | Type | Description |\n| --- | --- | --- |")
                    for name, ftype, desc, opt in item.fields:
                        t = ftype + ("?" if opt and not ftype.endswith("?") else "")
                        out.append(f"| `{name}` | {cell(self.type_md(t, page))} | {cell(desc)} |")
                    out.append("")
                methods = [i for i in items if i.kind == "function" and i.owner == item.name]
                if methods:
                    out.append("| Method | Summary |\n| --- | --- |")
                    for m in methods:
                        out.append(f"| [`{m.sep}{m.name}`](#{anchor(m)}) | {cell(first_sentence(m.text))} |")
                    out.append("")
            elif item.kind == "function":
                level = "####" if in_class and item.owner else "###"
                out.append(f"{level} `{signature(item)}` {{#{anchor(item)}}}\n")
                if item.text:
                    out.append(item.text + "\n")
                if item.params:
                    out.append("| Parameter | Type | Description |\n| --- | --- | --- |")
                    for name, ptype, desc, opt in item.params:
                        t = ptype + ("?" if opt and not ptype.endswith("?") else "")
                        out.append(f"| `{name}` | {cell(self.type_md(t, page))} | {cell(desc)} |")
                    out.append("")
                for rtype, rname, rdesc in item.returns:
                    label = f" `{rname}`" if rname else ""
                    desc = f": {rdesc}" if rdesc else ""
                    out.append(f"**Returns**{label} {self.type_md(rtype, page)}{desc}\n")
        _ = intro_done
        return "\n".join(out)


def relpath(target, start):
    """A link from page `start` to page `target`, both relative to src."""
    t = Path(target).with_suffix(".md")
    s = Path(start).parent
    parts = len(s.parts)
    return "../" * parts + str(t)


def build_index(pages):
    """Map every class and alias name to (page, anchor)."""
    index = {}
    for page, items in pages.items():
        for item in items:
            if item.kind in ("class", "alias"):
                index[item.name] = (page, item.name)
    return index


# ---------------------------------------------------------------- check

def source_text(src):
    """The Rust text a `-- source:` entry names: a whole file, or with
    `FILE#Type` only the `impl UserData for Type` block in it."""
    path, _, ty = src.partition("#")
    text = (ROOT / path).read_text()
    if not ty:
        return text
    m = re.search(r"impl(?:<[^>]*>)?\s+UserData\s+for\s+" + re.escape(ty) + r"\b[^{]*\{", text)
    if not m:
        raise SystemExit(f"{src}: no `impl UserData for {ty}`")
    depth, i = 1, m.end()
    while depth and i < len(text):
        depth += {"{": 1, "}": -1}.get(text[i], 0)
        i += 1
    return text[m.start():i]


def check():
    errors = []
    for path in sorted(LUA_DIR.glob("*.lua")):
        items = parse_file(path)
        for cls in (i for i in items if i.kind == "class" and i.source):
            registered = set()
            for src in cls.source:
                registered |= set(REGISTER.findall(source_text(src)))
            documented = {i.name for i in items
                          if i.kind == "function" and i.owner == cls.name}
            rel = path.relative_to(ROOT)
            for name in sorted(registered - documented):
                errors.append(f"{rel}: {cls.name}:{name} is registered in "
                              f"{' '.join(cls.source)} but not documented")
            for name in sorted(documented - registered):
                errors.append(f"{rel}: {cls.name}:{name} is documented but "
                              f"not registered in {' '.join(cls.source)}")
    for e in errors:
        print(e, file=sys.stderr)
    if errors:
        print(f"{len(errors)} Lua API mismatch(es)", file=sys.stderr)
        return 1
    print("Lua API definitions match the engine")
    return 0


# ---------------------------------------------------------------- mdbook

def preprocess():
    context, book = json.load(sys.stdin)
    src = Path(context["root"]) / context["config"]["book"].get("src", "src")

    chapters = []

    def walk(items):
        for entry in items:
            ch = entry.get("Chapter") if isinstance(entry, dict) else None
            if ch:
                chapters.append(ch)
                walk(ch.get("sub_items", []))

    walk(book.get("items", book.get("sections", [])))

    pages = {}
    for ch in chapters:
        if not ch.get("path"):
            continue
        for m in DIRECTIVE.finditer(ch["content"]):
            lua = LUA_DIR / m.group(1)
            pages[ch["path"]] = parse_file(lua)
    renderer = Renderer(build_index(pages))
    for ch in chapters:
        if ch.get("path") in pages:
            ch["content"] = DIRECTIVE.sub(
                lambda _m, p=ch["path"]: renderer.render(pages[p], p), ch["content"])
    json.dump(book, sys.stdout)
    return 0


def main(argv):
    if len(argv) >= 2 and argv[1] == "supports":
        return 0 if argv[2] == "html" else 1
    if len(argv) >= 2 and argv[1] == "check":
        return check()
    if len(argv) >= 3 and argv[1] == "render":
        path = Path(argv[2])
        page = "lua/" + path.with_suffix(".md").name
        pages = {page: parse_file(path)}
        print(Renderer(build_index(pages)).render(pages[page], page))
        return 0
    if len(argv) == 1:
        return preprocess()
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
