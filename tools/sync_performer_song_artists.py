#!/usr/bin/env python3
"""曲の「ライブ歌唱歴」(song_artists.role = 'performer') をセトリの出演者から作り直す。

ライブ歌唱歴はセトリの出演者 (setlist_performers) の和集合のはずだが、表の performer 行は
誰も更新しておらず、セトリを足しても直しても古いまま残っていた
(2026-10-13: Crossing! / Harmony 4 You の歌唱歴がロコだけ、成長Chu→LOVER!! は空)。

imas-core は 2026-10-13 から performer 行を読まずにセトリから組む
(imas-core/src/domain/snapshot_build.rs)。この表を揃えるのは、それより前のアプリ
(iOS 2.6.0 など、CloudKit の SongArtist をそのまま読む版) のため。

数えるのは披露に数える公演だけ (上映会・配信だけの公演は外す。core の
counts_as_performance と同じ)。

    python3 tools/sync_performer_song_artists.py                 # 差分を数えるだけ
    python3 tools/sync_performer_song_artists.py --apply         # master.sqlite を直す
    CLOUDKIT_KEY_ID=... python3 tools/sync_performer_song_artists.py --apply --push --production

足りない行は INSERT して、その曲の song_artists を CloudKit へ push する。
余分な行は master.sqlite から消し、CloudKit の削除待ち TSV
(tools/pending_cloudkit_deletions_performer_artists_<日付>.tsv) に書き出す。
CloudKit から消すのは seed_cloudkit.py --delete-file で行う (台帳にも 1 行足す)。
"""
from __future__ import annotations

import argparse
import sqlite3
import sys
from datetime import date
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS))
import apply_data  # noqa: E402

# core の NON_PERFORMANCE_PERFORMER_TYPES / NON_PERFORMANCE_VENUE_MODES と同じ。
DESIRED_SQL = """
SELECT DISTINCT si.song_id, sp.idol_id
FROM setlist_items si
JOIN shows sh ON sh.id = si.show_id
JOIN setlist_performers sp ON sp.setlist_item_id = si.id
JOIN songs s ON s.id = si.song_id
JOIN idols i ON i.id = sp.idol_id
WHERE IFNULL(sh.performer_type, '') <> 'screening'
  AND IFNULL(sh.venue_mode, '') <> 'online'
"""


def diff(conn):
    desired = set(conn.execute(DESIRED_SQL).fetchall())
    current = set(conn.execute(
        "SELECT song_id, idol_id FROM song_artists WHERE role = 'performer'").fetchall())
    return sorted(desired - current), sorted(current - desired)


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--apply", action="store_true", help="master.sqlite を直す")
    ap.add_argument("--push", action="store_true", help="足した曲の song_artists を CloudKit へ (要 --apply)")
    ap.add_argument("--production", action="store_true", help="push 先を Production に")
    ap.add_argument("--db", default=str(apply_data.DB_PATH))
    args = ap.parse_args()

    apply_data.ensure_db(args.db)
    conn = sqlite3.connect(args.db)
    missing, stale = diff(conn)
    print(f"足りない行: {len(missing)} ({len({s for s, _ in missing})} 曲) / "
          f"余分な行: {len(stale)} ({len({s for s, _ in stale})} 曲)")
    if not args.apply:
        print("(数えただけ。直すには --apply)")
        return
    if not missing and not stale:
        return

    conn.executemany(
        "INSERT OR IGNORE INTO song_artists (song_id, idol_id, role) VALUES (?, ?, 'performer')",
        missing)
    conn.executemany(
        "DELETE FROM song_artists WHERE song_id = ? AND idol_id = ? AND role = 'performer'",
        stale)
    conn.commit()
    conn.close()

    if stale:
        tsv = TOOLS / f"pending_cloudkit_deletions_performer_artists_{date.today():%Y%m%d}.tsv"
        lines = [
            "# ライブ歌唱歴 (song_artists.role = 'performer') のうち、セトリの出演者に無い行",
            f"# 使い方: python3 tools/seed_cloudkit.py --delete-file tools/{tsv.name} --production",
        ] + [f"SongArtist\tsong_artists-{s}-{i}-performer" for s, i in stale]
        tsv.write_text("\n".join(lines) + "\n", encoding="utf-8")
        print(f"削除待ち: {tsv.relative_to(TOOLS.parent)} ({len(stale)} 件。台帳にも 1 行足す)")

    if args.push and missing:
        rc = apply_data.push_cloudkit(
            {"song_artists": {s for s, _ in missing}}, args.production, args.db)
        if rc != 0:
            sys.exit(rc)
        print("✓ CloudKit push 完了")


if __name__ == "__main__":
    main()
