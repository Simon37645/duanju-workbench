#!/usr/bin/env python3
"""把 Word 文档转成 Markdown，供工作台的「知识包」使用。

工作台本身只认 Markdown —— 这个脚本是给你在导入前做一次性转换用的。

用法：
    python scripts/docx2md.py 输入.docx 输出目录/
    python scripts/docx2md.py 输入.docx 输出目录/ --kind style --name "我的风格圣经"
    python scripts/docx2md.py 某个目录/ 输出目录/          # 批量转换目录下所有 docx

转换内容：标题层级、段落、加粗、无序/有序列表、表格、图片占位（图片不导出）。
表格会转成 Markdown 表格；合并单元格会就地重复内容，属于有损转换。
"""

import argparse
import os
import re
import sys
import zipfile
from xml.etree import ElementTree as ET

NS = {"w": "http://schemas.openxmlformats.org/wordprocessingml/2006/main"}


def q(tag: str) -> str:
    return f"{{{NS['w']}}}{tag}"


def run_text(run) -> str:
    """一个 run 的文本，顺便把加粗/斜体转成 Markdown。"""
    parts = []
    for node in run.iter():
        if node.tag == q("t"):
            parts.append(node.text or "")
        elif node.tag == q("tab"):
            parts.append("  ")
        elif node.tag in (q("br"), q("cr")):
            parts.append("\n")
    text = "".join(parts)
    if not text:
        return ""
    rpr = run.find(q("rPr"))
    if rpr is not None:
        if rpr.find(q("b")) is not None and text.strip():
            text = f"**{text}**"
        elif rpr.find(q("i")) is not None and text.strip():
            text = f"*{text}*"
    return text


def para_text(p) -> str:
    return "".join(run_text(r) for r in p.findall(q("r")))


def para_style(p) -> str:
    ppr = p.find(q("pPr"))
    if ppr is None:
        return ""
    st = ppr.find(q("pStyle"))
    return st.get(q("val")) if st is not None else ""


def is_list(p) -> str:
    """返回 'bullet' / 'number' / ''"""
    ppr = p.find(q("pPr"))
    if ppr is None:
        return ""
    numpr = ppr.find(q("numPr"))
    if numpr is None:
        return ""
    ilvl = numpr.find(q("ilvl"))
    numid = numpr.find(q("numId"))
    if numid is None:
        return ""
    depth = int(ilvl.get(q("val")) or 0) if ilvl is not None else 0
    # 无法可靠区分项目符号与编号，用 numId 的奇偶做个近似：绝大多数文档够用
    marker = "1." if int(numid.get(q("val")) or 0) % 2 == 0 else "-"
    return "  " * depth + marker + " "


def heading_level(style: str) -> int:
    if not style:
        return 0
    if style.lower().startswith("title"):
        return 1
    m = re.search(r"(\d+)", style)
    if style.lower().startswith("heading") and m:
        return min(int(m.group(1)), 6)
    return 0


def table_to_md(tbl) -> str:
    rows = []
    for tr in tbl.findall(q("tr")):
        cells = []
        for tc in tr.findall(q("tc")):
            text = " ".join(
                para_text(p).strip() for p in tc.findall(q("p"))
            ).strip()
            cells.append(text.replace("|", "\\|"))
        rows.append(cells)
    if not rows:
        return ""
    width = max(len(r) for r in rows)
    rows = [r + [""] * (width - len(r)) for r in rows]
    out = ["| " + " | ".join(rows[0]) + " |", "|" + "---|" * width]
    for r in rows[1:]:
        out.append("| " + " | ".join(r) + " |")
    return "\n".join(out)


def looks_like_heading(text: str) -> bool:
    """有的文档不套 Word 标题样式，而是整行加粗当地用。

    判断依据：整段就是一个加粗 run、长度短、结尾不带句读。
    这个启发式是 opt-in 的（--guess-headings），因为纯加粗的短句也可能是强调。
    """
    if len(text) > 34:
        return False
    if not (text.startswith("**") and text.endswith("**")):
        return False
    inner = text.strip("*").strip()
    if not inner or inner.endswith(("。", "！", "？", "：", ".", "!", "?", ":", "，", ",")):
        return False
    return True


def convert(path: str, guess_headings: bool = False) -> str:
    with zipfile.ZipFile(path) as z:
        xml = z.read("word/document.xml")
    root = ET.fromstring(xml)
    body = root.find(q("body"))
    if body is None:
        return ""

    out = []
    for node in body:
        if node.tag == q("p"):
            style = para_style(node)
            text = para_text(node).strip()
            if not text:
                continue
            lvl = heading_level(style)
            if lvl:
                out.append(f"\n{'#' * lvl} {text}\n")
                continue
            bullet = is_list(node)
            if bullet:
                out.append(bullet + text)
                continue
            if guess_headings and looks_like_heading(text):
                out.append("## " + text.strip("*").strip())
                continue
            out.append(text)
        elif node.tag == q("tbl"):
            md = table_to_md(node)
            if md:
                out.append("\n" + md + "\n")

    text = "\n\n".join(out)
    # 收紧列表之间的空行
    text = re.sub(r"\n{3,}", "\n\n", text)
    text = re.sub(r"(\n(?:[-*]|\d+\.) [^\n]+)\n\n(?=(?:[-*]|\d+\.) )", r"\1\n", text)
    return text.strip() + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("source", help="docx 文件或包含 docx 的目录")
    ap.add_argument("outdir", help="输出目录")
    ap.add_argument("--kind", default="reference",
                    help="methodology / style / checklist / reference")
    ap.add_argument("--name", default="", help="知识包显示名，默认用文件名")
    ap.add_argument("--summary", default="", help="一句话摘要")
    ap.add_argument("--guess-headings", action="store_true",
                    help="文档没套标题样式时，把「整行加粗的短句」当成小标题")
    args = ap.parse_args()

    if os.path.isdir(args.source):
        files = [
            os.path.join(args.source, f)
            for f in sorted(os.listdir(args.source))
            if f.lower().endswith(".docx") and not f.startswith("~$")
        ]
    else:
        files = [args.source]

    if not files:
        print("没有找到 docx", file=sys.stderr)
        return 1

    os.makedirs(args.outdir, exist_ok=True)
    for f in files:
        stem = os.path.splitext(os.path.basename(f))[0]
        try:
            body = convert(f, args.guess_headings)
        except Exception as e:  # noqa: BLE001
            print(f"  跳过 {os.path.basename(f)}：{e}", file=sys.stderr)
            continue
        name = args.name or stem
        front = "---\n"
        front += f"name: {name}\n"
        front += f"kind: {args.kind}\n"
        if args.summary:
            front += f"summary: {args.summary}\n"
        front += "---\n\n"
        dest = os.path.join(args.outdir, f"{stem}.md")
        with open(dest, "w", encoding="utf-8") as fh:
            fh.write(front + body)
        print(f"  {os.path.basename(f)}  →  {dest}  ({len(body)} 字)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
