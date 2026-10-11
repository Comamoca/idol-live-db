#!/usr/bin/env python3
"""
fetch_call_names_from_nicodic.py

ニコニコ大百科の「呼称表」(誰が誰をどう呼ぶか) を読み、こちらのアイドル id に突き合わせて
data/personas/call_names.json に足す。既存の im@sparql 由来の行 (tools/fetch_call_names_from_sparql.py)
はそのまま残し、行の `source` で出どころを分ける (im@sparql の行は im@sparql の URL、
大百科の行は呼称表の URL)。同じ from/to に両方ある場合は両方残す。

- 取るのは各アイドルの「一人称」(from == to の行)・「プロデューサー」(to = "producer")・
  「他のアイドル」への呼び方。複数の呼び方 (「、」や改行で区切られたもの) は全部別の行にする。
- 「呼ばれ方」の列は、相手側の「呼び方」と同じ情報なので取らない (列がずれている表もある)。
- 突き合わせはアイドルの名前とエイリアス (空白・全角半角の違いは無視。同名が複数いれば落とす)。
  当たらなかったもの (表記ゆれ・マスタにいない人) は
  data/research/personas_20261012/unmatched_call_names.csv に出す。
- 何度回しても同じ結果になる (大百科の行は毎回作り直し、並びは固定)。
- 読んだ HTML は保存しない。--cache-dir を渡したときだけ、そのディレクトリに置く
  (リポジトリの外を指定すること)。

usage:
  python3 tools/fetch_call_names_from_nicodic.py
  python3 tools/fetch_call_names_from_nicodic.py --cache-dir /tmp/nicodic   # HTML を残す・再利用する
"""
import argparse
import csv
import json
import re
import sqlite3
import sys
import unicodedata
import urllib.parse
import urllib.request
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DB = ROOT / "ImasLiveDB" / "Resources" / "master.sqlite"
OUT = ROOT / "data" / "personas" / "call_names.json"
UNMATCHED = ROOT / "data" / "research" / "personas_20261012" / "unmatched_call_names.csv"
SPARQL_SOURCE = "https://sparql.crssnky.xyz/spql/imas/query"
NICODIC = "https://dic.nicovideo.jp/t/a/"

# (記事名, 表の URL に使う記事名)。記事名は大百科のタイトルのとおり。
PAGES = [
    "アイドルマスター:呼称表",
    "アイドルマスター シンデレラガールズ:呼称表",
    "アイドルマスター ミリオンライブ!:呼称表",
    "アイドルマスターsidem:呼称表",
    "アイドルマスター シャイニーカラーズ:呼称表",
    "学園アイドルマスター:呼称表",
]

# 表の表記とマスタの名前が違うもの (正規化後の表記 → idol id)。
ALIASES = {
    "エミリー": "ml_エミリースチュアート",
}

UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120 Safari/537.36"
LINE_BREAK_TAGS = {"br", "div", "p", "li"}


def page_url(title):
    """出典に書く URL。オーナーが示した形 (空白と : は %エンコード、日本語はそのまま)。"""
    return NICODIC + title.replace(" ", "%20").replace(":", "%3A").replace("!", "%21")


def fetch_url(title):
    return NICODIC + urllib.parse.quote(title)


def read_page(title, cache_dir):
    cache = None
    if cache_dir:
        cache = Path(cache_dir) / (urllib.parse.quote(title, safe="") + ".html")
        if cache.exists():
            return cache.read_text(encoding="utf-8")
    req = urllib.request.Request(fetch_url(title), headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=60) as r:
        html = r.read().decode("utf-8", errors="replace")
    if cache:
        cache.parent.mkdir(parents=True, exist_ok=True)
        cache.write_text(html, encoding="utf-8")
    return html


class TableParser(HTMLParser):
    """HTML の表を [表][行][セル] (セルは改行を残した文字列) に直す。入れ子の表は見ない。"""

    def __init__(self):
        super().__init__(convert_charrefs=True)
        self.tables = []
        self.depth = 0
        self.row = None
        self.cell = None

    def handle_starttag(self, tag, attrs):
        if tag == "table":
            self.depth += 1
            if self.depth == 1:
                self.tables.append([])
        elif self.depth == 1:
            if tag == "tr":
                self.row = []
                self.tables[-1].append(self.row)
            elif tag in ("td", "th") and self.row is not None:
                self.cell = []
                self.row.append(self.cell)
            elif tag in LINE_BREAK_TAGS and self.cell is not None:
                self.cell.append("\n")
            elif tag == "sup" and self.cell is not None:
                self.cell.append("\x00")

    def handle_endtag(self, tag):
        if tag == "table":
            self.depth -= 1
        elif self.depth == 1:
            if tag in ("td", "th"):
                self.cell = None
            elif tag == "tr":
                self.row = None
            elif tag in ("div", "p", "li") and self.cell is not None:
                self.cell.append("\n")
            elif tag == "sup" and self.cell is not None:
                self.cell.append("\x00")

    def handle_data(self, data):
        if self.depth == 1 and self.cell is not None:
            self.cell.append(data.replace("\n", " "))  # 改行は <br> など タグでだけ入れる


def parse_tables(html):
    p = TableParser()
    p.feed(html)
    tables = []
    for t in p.tables:
        rows = []
        for r in t:
            cells = []
            for c in r:
                text = "".join(c)
                text = re.sub(r"\x00[^\x00]*\x00", "", text)  # 脚注 <sup>[1]</sup>
                text = text.replace("\xa0", " ")
                # 空行も残す (相手と呼び方を行の位置で突き合わせる表があり、呼び方が空の相手は空行になっている)
                cells.append([ln.strip() for ln in text.split("\n")])
            rows.append(cells)
        tables.append(rows)
    return tables


def text_of(cell):
    return " ".join(x for x in cell if x).strip()


def split_forms(cell):
    """セルの行 → 呼び方のリスト。「、」で区切られた複数形も分ける (括弧の中の「、」は分けない)。"""
    out = []
    for line in cell:
        line = line.strip()
        line = re.sub(r"^[■□▲△◆◇●○]+", "", line) if line[:1] in "■□▲△◆◇●" else line
        line = line.rstrip(" /").strip()
        depth, cur = 0, ""
        parts = []
        for ch in line:
            if ch in "（(":
                depth += 1
            elif ch in "）)":
                depth = max(0, depth - 1)
            if ch == "、" and depth == 0:
                parts.append(cur)
                cur = ""
            else:
                cur += ch
        parts.append(cur)
        for p in parts:
            p = p.strip()
            if p and p not in ("-", "－", "—"):
                out.append(p)
    return out


def norm(name):
    return re.sub(r"\s+", "", unicodedata.normalize("NFKC", name)).casefold()


def load_master():
    con = sqlite3.connect(DB)
    keys = {}

    def add(key, idol_id):
        keys.setdefault(norm(key), set()).add(idol_id)

    for idol_id, name, aliases in con.execute("SELECT id, name, aliases FROM idols WHERE is_external = 0"):
        add(name, idol_id)
        for a in re.split(r"[|,]", aliases or ""):
            if a.strip():
                add(a, idol_id)
    # 同名のアイドルがいる名前は突き合わせに使わない (取り違えるより落とす)。
    unique = {k: next(iter(v)) for k, v in keys.items() if len(v) == 1}
    for k, v in ALIASES.items():
        unique[norm(k)] = v
    return unique


def parse_arrow_lines(cells):
    """「相手 → 呼び方、呼び方」の行 → [(相手の名前, [呼び方])]。矢印の無い行 (注記) は捨てる。"""
    out = []
    for cell in cells:
        for line in cell:
            line = re.sub(r"^[■□▲△◆◇●]+", "", line.strip()).rstrip(" /").strip()
            if "→" not in line:
                continue
            target, called = line.split("→", 1)
            out.append((target.strip(), split_forms([called])))
    return out


def extract(tables):
    """表 → [(呼ぶ側の名前, 呼ばれる側 ('self' / 'producer' / 名前), [呼び方], 備考)]。

    備考は取り込めなかった理由 ("" なら問題なし)。
    """
    records = []
    for rows in tables:
        head = " ".join(text_of(c) for r in rows[:3] for c in r)
        if "他のキャラクター" in head or "他のアイドル" in head:
            # 1 行 1 人: 呼ぶ側 / 一人称 / プロデューサー / 他の人 (複数列に割れることがある)
            for r in rows:
                if len(r) < 3 or text_of(r[0]) in ("呼ぶ側", "一人称") or not r[0]:
                    continue
                who = text_of(r[0])
                if who == "プロデューサー":
                    continue
                records.append((who, "self", split_forms(r[1]), ""))
                records.append((who, "producer", split_forms(r[2]), ""))
                for target, called in parse_arrow_lines(r[3:]):
                    records.append((who, target, called, ""))
        elif any(text_of(c) == "呼び方" for r in rows[:2] for c in r):
            # 見出しの行 (呼び方) のあと、1 人につき 一人称 / プロデューサー / 相手の並び の行が続く
            who = None
            for r in rows:
                if len(r) >= 3 and text_of(r[1]) == "一人称":
                    who = text_of(r[0])
                    if who == "プロデューサー":
                        who = None
                        continue
                    records.append((who, "self", split_forms(r[2]), ""))
                elif who and len(r) >= 2 and text_of(r[0]) == "プロデューサー":
                    records.append((who, "producer", split_forms(r[1]), ""))
                elif who and len(r) >= 2 and text_of(r[1]) != "相手":
                    names, calls = list(r[0]), list(r[1])
                    while names and not names[-1]:
                        names.pop()
                    while calls and not calls[-1]:
                        calls.pop()
                    if len(calls) > len(names):
                        # 先頭に余計な空行が入っている表がある。取って数が合うときだけそうする。
                        k = next((i for i, c in enumerate(calls) if c), len(calls))
                        if len(calls) - k <= len(names):
                            calls = calls[k:]
                    calls += [""] * (len(names) - len(calls))  # 末尾の相手に呼び方が無いと、行ごと省かれている
                    if len(names) != len(calls):
                        for n in names:
                            if n:
                                records.append((who, n, [], "並びのずれ (相手の数と呼び方の数が合わない)"))
                        continue
                    for n, c in zip(names, calls):
                        if n and c:
                            records.append((who, n, split_forms([c]), ""))
    return records


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cache-dir", help="読んだ HTML を置く・再利用するディレクトリ (リポジトリの外)")
    args = ap.parse_args()
    if args.cache_dir and ROOT in Path(args.cache_dir).resolve().parents:
        sys.exit("--cache-dir はリポジトリの外を指定してください")

    unique = load_master()
    new_rows = set()
    unmatched = set()
    per_page = {}
    for title in PAGES:
        url = page_url(title)
        records = extract(parse_tables(read_page(title, args.cache_dir)))
        ok = bad = 0
        for who, target, called, note in records:
            if not called and not note:
                continue
            src = unique.get(norm(who))
            if target == "self":
                dest = src
            elif target == "producer":
                dest = "producer"
            else:
                dest = unique.get(norm(target))
            reason = note or (
                "呼ぶ側がマスタに無い・同名" if not src else "相手がマスタに無い・同名" if not dest else ""
            )
            if reason:
                bad += 1
                label = {"self": "(自分)", "producer": "プロデューサー"}.get(target, target)
                unmatched.add((title, who, label, "／".join(called), reason))
                continue
            for c in called:
                new_rows.add((src, dest, c, url))
            ok += 1
        per_page[title] = (ok, bad)

    existing = json.loads(OUT.read_text(encoding="utf-8")) if OUT.exists() else []
    rows = set()
    for r in existing:
        source = r.get("source", SPARQL_SOURCE)
        if source.startswith(NICODIC):
            continue  # 大百科の行は毎回作り直す
        rows.add((r["from"], r["to"], r["called"], source))
    sparql_rows = set(rows)
    rows |= new_rows
    out = [{"from": f, "to": t, "called": c, "source": s} for f, t, c, s in sorted(rows)]
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(out, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")

    UNMATCHED.parent.mkdir(parents=True, exist_ok=True)
    with UNMATCHED.open("w", encoding="utf-8", newline="") as f:
        f.write("﻿")
        w = csv.writer(f, lineterminator="\r\n")
        w.writerow(["ページ", "呼ぶ側", "呼ばれる側", "呼び方", "理由"])
        for row in sorted(unmatched):
            w.writerow(row)

    # im@sparql との突き合わせ: 同じ from/to に両方ある組で、呼び方の集合が違うもの。
    by_pair_sparql, by_pair_nico = {}, {}
    for f, t, c, s in sparql_rows:
        by_pair_sparql.setdefault((f, t), set()).add(c)
    for f, t, c, s in new_rows:
        by_pair_nico.setdefault((f, t), set()).add(c)
    both = [k for k in by_pair_nico if k in by_pair_sparql]
    same = [k for k in both if by_pair_nico[k] == by_pair_sparql[k]]
    disjoint = [k for k in both if not (by_pair_nico[k] & by_pair_sparql[k])]
    for title, (ok, bad) in per_page.items():
        print(f"{title}: 取り込み {ok} 組 / 突き合わせできず {bad} 組")
    print(f"call_names.json: {len(out)} 行 (大百科 {len(new_rows)} 行 + im@sparql {len(sparql_rows)} 行)")
    print(f"im@sparql と両方ある組 {len(both)}: 一致 {len(same)} / 一部重なる {len(both) - len(same) - len(disjoint)} / 重ならない {len(disjoint)}")
    for k in sorted(disjoint)[:8]:
        print("  重ならない:", k, "大百科", sorted(by_pair_nico[k]), "im@sparql", sorted(by_pair_sparql[k]))
    print(f"突き合わせできず {len(unmatched)} 件 → {UNMATCHED.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
