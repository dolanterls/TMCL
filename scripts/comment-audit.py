#!/usr/bin/env python3
"""Read-only audit of .rs comments against docs/comment-style.md.

This script NEVER writes to source files. It opens them for reading, reports
what breaks the rule, and exits. Every rewrite is done by hand.

    python scripts/comment-audit.py                    # summary table
    python scripts/comment-audit.py --md out.md        # per-block checklist
    python scripts/comment-audit.py --md out.md src/tui

A "block" is one run of adjacent `//` lines, so each checklist entry is a unit
that can be read, judged and rewritten as a whole.
"""

import argparse
import glob
import re

# `X, used to <verb>` is a postmodifier ("used for"), not a past tense; the
# historical sense follows a subject directly ("the verdict used to be").
HISTORY = re.compile(r"(?<!, )\bused to\b|\boriginally\b", re.I)
# `not just X` and `just as X` are ordinary English; only the dismissive use counts.
FILLER = re.compile(
    r"\b(actually|really|simply|obviously)\b|(?<!not )\bjust\b(?! as\b)", re.I
)
PUNCHLINE = re.compile(
    r"\b(that is (where|why|what)|this is (where|why|the (whole )?point)|"
    r"which is (why|where))\b",
    re.I,
)
METAPHOR = re.compile(
    r"\b(ghosts?|soup|leaks?|leaked|wallpaper|swallow(s|ed)?|floating|"
    r"bite|wound|smell)\b",
    re.I,
)
EM_DASH = "—"
QUOTED = re.compile(r"`[^`]*`|\"[^\"]*\"")
MOD_HEADER_CAP = 6
DOC_BLOCK_CAP = 6
DENSITY_CAP = 0.20
SMALL_FILE = 150


def comment_blocks(lines):
    """Yield (first_line_no, block_lines) for each run of adjacent // lines."""
    i = 0
    while i < len(lines):
        if lines[i].strip().startswith("//"):
            j = i
            while j < len(lines) and lines[j].strip().startswith("//"):
                j += 1
            yield i + 1, lines[i:j]
            i = j
        else:
            i += 1


def block_marks(block):
    text = "\n".join(block)
    body = [line.strip() for line in block if line.strip() not in ("//", "///", "//!")]
    # An em dash inside backticks or double quotes is a literal the comment
    # quotes back at the reader, not a rhetorical break.
    prose = [QUOTED.sub("", line) for line in body]
    return {
        "dash": sum(line.count(EM_DASH) % 2 for line in prose),
        "metaphor": len(METAPHOR.findall(text)),
        "punchline": len(PUNCHLINE.findall(text)),
        "history": len(HISTORY.findall(text)),
        "filler": len(FILLER.findall(text)),
    }


def audit(path):
    lines = open(path, encoding="utf-8").read().splitlines()
    total = len(lines)
    comment = sum(1 for line in lines if line.strip().startswith("//"))
    header = 0
    long_doc = []
    flagged = []

    for start, block in comment_blocks(lines):
        marks = block_marks(block)
        if sum(marks.values()):
            flagged.append((start, block, marks))
        stripped = block[0].strip()
        for kind in ("//!", "///"):
            if stripped.startswith(kind):
                if kind == "//!":
                    header = max(header, len(block))
                elif len(block) > DOC_BLOCK_CAP:
                    long_doc.append(start)

    density = comment / total if total >= SMALL_FILE else 0.0
    shape = (
        len(long_doc)
        + (1 if density > DENSITY_CAP else 0)
        + (1 if header > MOD_HEADER_CAP else 0)
    )
    return dict(
        path=path,
        total=total,
        density=density,
        header=header,
        long_doc=long_doc,
        shape=shape,
        flagged=flagged,
        style_total=sum(sum(m.values()) for _, _, m in flagged),
    )


def collect(roots):
    rows = []
    for root in roots:
        for path in glob.glob(f"{root}/**/*.rs", recursive=True):
            row = audit(path)
            if row["style_total"] or row["shape"]:
                rows.append(row)
    rows.sort(key=lambda r: (-r["style_total"], -r["shape"]))
    return rows


def print_table(rows):
    print(f"{'file':46} {'lines':>6} {'density':>8} {'blocks':>7} "
          f"{'dash':>5} {'meta':>5} {'punch':>6} {'hist':>5} {'filler':>7}")
    print("-" * 108)
    for row in rows:
        marks = [block_marks(b) for _, b, _ in row["flagged"]]
        total = {k: sum(m[k] for m in marks) for k in marks[0]} if marks else {}
        print(
            f"{row['path']:46} {row['total']:6} {row['density']:>7.0%} "
            f"{len(row['flagged']):7} {total.get('dash', 0):5} "
            f"{total.get('metaphor', 0):5} {total.get('punchline', 0):6} "
            f"{total.get('history', 0):5} {total.get('filler', 0):7}"
        )
    blocks = sum(len(r["flagged"]) for r in rows)
    marks = sum(r["style_total"] for r in rows)
    print(f"\n{len(rows)} files, {blocks} comment blocks, {marks} markers")


def trigger_line(start, block):
    """The first line of a block that trips a rule, with its line number."""
    for offset, line in enumerate(block):
        body = line.strip()
        if body in ("//", "///", "//!"):
            continue
        if (
            QUOTED.sub("", body).count(EM_DASH) % 2
            or METAPHOR.search(body)
            or PUNCHLINE.search(body)
            or HISTORY.search(body)
            or FILLER.search(body)
        ):
            return start + offset, body.lstrip("/!").strip()
    return start, block[0].strip().lstrip("/!").strip()


def print_markdown(rows, out):
    kinds = {
        "dash": "破折号",
        "metaphor": "比喻",
        "punchline": "点题",
        "history": "历史",
        "filler": "填充词",
    }
    blocks = sum(len(r["flagged"]) for r in rows)
    lines = [
        "# 注释逐条清理清单",
        "",
        "由 `scripts/comment-audit.py` 做只读扫描生成。每条对应一个连续的注释块，",
        "可整体判断、整体重写。判决标准见 `docs/comment-style.md`。",
        "",
        "本清单只用于定位，不得据此批量替换。每一条都需人工读代码后重写。",
        "",
        f"共 {len(rows)} 个文件、{blocks} 个待处理注释块。",
        "",
    ]
    for row in rows:
        if not row["flagged"]:
            continue
        shown = row["path"].replace("\\", "/")
        lines.append(f"## {shown}  ({len(row['flagged'])} 块)")
        lines.append("")
        for start, block, marks in row["flagged"]:
            tags = " · ".join(f"{kinds[k]}×{v}" for k, v in marks.items() if v)
            at, text = trigger_line(start, block)
            if len(text) > 90:
                text = text[:88] + "…"
            span = f"L{start}-{start + len(block) - 1}"
            lines.append(f"- [ ] **L{at}**（{span}）— {tags}")
            lines.append(f"      `{text}`")
        lines.append("")
    open(out, "w", encoding="utf-8").write("\n".join(lines))
    print(f"wrote {out}: {len(rows)} files, {blocks} blocks")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--md", metavar="PATH", help="write a per-block checklist")
    parser.add_argument("roots", nargs="*", default=None)
    args = parser.parse_args()
    rows = collect(args.roots or ["src", "tests"])
    if args.md:
        print_markdown(rows, args.md)
    else:
        print_table(rows)


if __name__ == "__main__":
    main()
