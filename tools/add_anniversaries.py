#!/usr/bin/env python3
"""add_anniversaries.py — 年表の節目 (anniversaries) を足す。

Usage:
    python3 tools/add_anniversaries.py            # 何が入るか見るだけ
    python3 tools/add_anniversaries.py --apply    # master.sqlite に反映

出典は各シリーズの Wikipedia 沿革 + 公式ニュース。公式サイトは RSS も sitemap も
無く、ニュース一覧は 403 で記事は内部 CMS API 越しなので、全記事を舐める方式は取らない。
整理済みの沿革から拾い、日付は公式記事で裏を取る。

粒度は既存21件に揃える。「稼働・配信開始」「サービス終了」「TVアニメ」「劇場版」
「受賞」だけを入れ、機能追加やイベント単発 (○○コラボ / ガチャ更新) は入れない。
年表は俯瞰するためのもので、細かい出来事を並べると節目が埋もれる。

⚠️ anniversaries は CloudKit 同期の対象外。同梱 master.sqlite の data_version を
   上げた reseed でしか既存ユーザーに届かない。
"""

import argparse
import os
import sqlite3

from lib import masterdb

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(HERE)
DB_PATH = os.path.join(REPO, "ImasLiveDB", "Resources", "master.sqlite")
DUMP_PATH = os.path.join(REPO, "db", "master.sql")

# (brand_id, date, label, kind[, 出典 URL])
# kind: service_start / app_start / anime_start / movie_release / cd_debut
#     / service_end / award
ENTRIES = [
    # ---- 765AS: 家庭用移植とシリーズの節目 ----
    ("765as", "2007-01-25", "「THE IDOLM@STER」(Xbox 360)発売", "service_start"),
    ("765as", "2011-02-24", "「アイドルマスター2」発売", "service_start"),
    ("765as", "2025-03-26", "「アイドルマスター ツアーズ」稼働開始", "service_start"),

    # ---- シンデレラガールズ ----
    ("cg", "2014-11-17", "モバマス アプリ版配信開始", "app_start"),
    ("cg", "2015-07-10", "TVアニメ第2期放映開始", "anime_start"),
    ("cg", "2023-03-30", "モバマス サービス終了", "service_end"),
    ("cg", "2025-08-21", "デレステ(DMM版) サービス終了", "service_end"),

    # ---- ミリオンライブ ----
    ("ml", "2013-04-24", "CDシリーズ「LIVE THE@TER PERFORMANCE」開始", "cd_debut"),
    ("ml", "2018-03-19", "グリマス サービス終了", "service_end"),
    ("ml", "2026-07-15", "52週連続CD「SPECIAL SOLO RECORDS」完走・ギネス世界記録達成", "award"),

    # ---- SideM ----
    ("sidem", "2014-02-28", "モバゲー版SideM サービス開始(初回)", "service_start"),
    ("sidem", "2023-01-05", "モバゲー版SideM サービス終了", "service_end"),

    # ---- シャイニーカラーズ ----
    ("sc", "2019-03-13", "シャニマス アプリ版配信開始", "app_start"),

    # ---- 2026-10-13 追加: 家庭用・携帯機・VR・アプリ・アニメ・CD の起点日 ----
    # 5 つ目は日付を確かめた出典。Wikipedia は公式・ゲーム媒体と食い違わないものだけ使う。
    ("765as", "2008-02-28", "「アイドルマスター ライブフォーユー！」(Xbox 360)発売", "service_start",
     "https://game.watch.impress.co.jp/docs/20080228/l4u.htm"),
    ("765as", "2011-10-27", "「アイドルマスター2」(PS3)発売", "service_start",
     "https://ja.wikipedia.org/wiki/THE_IDOLM@STER_2"),
    ("765as", "2012-03-30", "「アイドルマスターモバイル i」(iOS)配信開始", "app_start",
     "https://hatenanews.com/articles/201204/8257"),
    ("765as", "2012-10-25", "「アイドルマスター シャイニーフェスタ」(PSP)発売", "service_start",
     "https://game.watch.impress.co.jp/docs/news/542235.html"),
    ("765as", "2013-01-01", "Webアニメ「ぷちます！」配信開始", "anime_start",
     "https://ja.wikipedia.org/wiki/ぷちます!_-PETIT_IDOLM@STER-"),
    ("765as", "2014-05-15", "「アイドルマスター ワンフォーオール」(PS3)発売", "service_start",
     "https://www.4gamer.net/games/238/G023836/20131219015/"),
    ("765as", "2016-01-18", "「アイドルマスターモバイル i」サービス終了", "service_end",
     "https://game.watch.impress.co.jp/docs/news/730994.html"),
    ("765as", "2016-07-28", "「アイドルマスター プラチナスターズ」(PS4)発売", "service_start",
     "https://ja.wikipedia.org/wiki/アイドルマスター_プラチナスターズ"),
    ("765as", "2017-12-21", "「アイドルマスター ステラステージ」(PS4)発売", "service_start",
     "https://blog.ja.playstation.com/2017/09/20/20170920-imas/"),
    ("876", "2009-10-14", "CDシリーズ「DREAM SYMPHONY」開始", "cd_debut",
     "https://dengekionline.com/elem/000/000/195/195532/"),
    ("961", "2011-11-23", "Jupiter 初CD「THE IDOLM@STER Jupiter」発売", "cd_debut",
     "https://ja.wikipedia.org/wiki/THE_IDOLM@STER_Jupiter"),
    ("other", "2007-04-02", "TVアニメ「アイドルマスター XENOGLOSSIA」放映開始", "anime_start",
     "https://ja.wikipedia.org/wiki/アイドルマスター_XENOGLOSSIA"),
    ("other", "2021-01-21", "「アイドルマスター ポップリンクス」配信開始", "app_start",
     "https://www.4gamer.net/games/537/G053749/20210121061/"),
    ("other", "2021-10-14", "「アイドルマスター スターリットシーズン」発売", "service_start",
     "https://www.4gamer.net/games/493/G049394/20211014020/"),
    ("other", "2022-07-21", "「アイドルマスター ポップリンクス」サービス終了", "service_end",
     "https://app.famitsu.com/20220420_1940889/"),

    ("cg", "2012-04-18", "CDシリーズ「CINDERELLA MASTER」開始", "cd_debut",
     "https://cinderella-library.idolmaster-official.jp/history/detail/?p=20120418_1"),
    ("cg", "2016-03-30", "CDシリーズ「STARLIGHT MASTER」開始", "cd_debut",
     "https://ja.wikipedia.org/wiki/THE_IDOLM@STER_CINDERELLA_GIRLS_STARLIGHT_MASTER"),
    ("cg", "2016-10-13", "「シンデレラガールズ ビューイングレボリューション」(PS VR)発売", "service_start",
     "https://ja.wikipedia.org/wiki/アイドルマスター_シンデレラガールズ_ビューイングレボリューション"),
    ("cg", "2017-04-04", "TVアニメ「シンデレラガールズ劇場」放映開始", "anime_start",
     "https://dengekionline.com/elem/000/001/471/1471300"),
    ("cg", "2017-10-03", "TVアニメ「シンデレラガールズ劇場」2nd SEASON放映開始", "anime_start",
     "https://ja.wikipedia.org/wiki/シンデレラガールズ劇場"),
    ("cg", "2018-07-03", "TVアニメ「シンデレラガールズ劇場」3rd SEASON放映開始", "anime_start",
     "https://natalie.mu/comic/news/285789"),
    ("cg", "2019-04-02", "TVアニメ「シンデレラガールズ劇場」CLIMAX SEASON放映開始", "anime_start",
     "https://cinderella-library.idolmaster-official.jp/history/detail/?p=20190402_2"),
    ("cg", "2019-09-03", "「シンデレラガールズ スターライトスポット」(VR)配信開始", "app_start",
     "https://ja.wikipedia.org/wiki/アイドルマスター_シンデレラガールズ_スターライトステージ"),
    ("cg", "2020-03-24", "「シンデレラガールズ劇場 Extra Stage」配信開始", "anime_start",
     "https://cinderella-library.idolmaster-official.jp/history/detail/?p=20200324_1"),
    ("cg", "2021-09-03", "デレステ(DMM版)配信開始", "service_start",
     "https://ja.wikipedia.org/wiki/アイドルマスター_シンデレラガールズ_スターライトステージ"),
    ("cg", "2023-04-06", "TVアニメ「U149」放映開始", "anime_start",
     "https://ja.wikipedia.org/wiki/アイドルマスター_シンデレラガールズ_U149"),

    ("ml", "2017-07-26", "CDシリーズ「MILLION THE@TER GENERATION」開始", "cd_debut",
     "https://ja.wikipedia.org/wiki/THE_IDOLM@STER_MILLION_THE@TER_GENERATION"),
    ("ml", "2019-08-30", "ミリシタ 韓国語版・繁体字版 配信開始", "app_start",
     "https://ja.wikipedia.org/wiki/アイドルマスター_ミリオンライブ!_シアターデイズ"),
    ("ml", "2022-01-28", "ミリシタ 韓国語版・繁体字版 サービス終了", "service_end",
     "https://ja.wikipedia.org/wiki/アイドルマスター_ミリオンライブ!_シアターデイズ"),

    ("sidem", "2015-04-15", "CDシリーズ「ST@RTING LINE」開始", "cd_debut",
     "https://ja.wikipedia.org/wiki/THE_IDOLM@STER_SideM_ST@RTING_LINE"),
    ("sidem", "2018-10-09", "TVアニメ「理由あってMini!」放映開始", "anime_start",
     "https://animeanime.jp/article/2018/08/21/39594.html"),
    ("sidem", "2021-08-31", "エムステ サービス終了", "service_end",
     "https://x.com/SideM_LOS/status/1432524839537971202"),
    ("sidem", "2021-10-06", "サイスタ(GROWING STARS)配信開始", "app_start",
     "https://www.4gamer.net/games/558/G055849/20230404034/"),
    ("sidem", "2023-07-31", "サイスタ(GROWING STARS)サービス終了", "service_end",
     "https://www.4gamer.net/games/558/G055849/20230404034/"),

    ("sc", "2018-06-06", "CDシリーズ「BRILLI@NT WING」開始", "cd_debut",
     "https://shinycolors.lantis.jp/releaseinfo/lacm-14781/"),
    ("sc", "2023-10-27", "シャニアニ第1章 劇場先行上映", "movie_release",
     "https://eeo.today/media/2023/10/17/111756/"),
    ("sc", "2024-07-05", "シャニアニ2nd 劇場先行上映", "movie_release",
     "https://www.famitsu.com/news/202403/03336523.html"),

    ("gakuen", "2024-08-07", "初のCDシングル発売", "cd_debut",
     "https://www.famitsu.com/article/202405/4159"),
    ("gakuen", "2025-03-18", "学マス(DMM版)配信開始", "service_start",
     "https://www.famitsu.com/article/202503/36900"),
]

SOURCES = {
    "award": "https://idolmaster-official.jp/news/01_19422",
}


def entry_id(brand: str, date: str, kind: str) -> str:
    return f"ann_{brand}_{date.replace('-', '')}_{kind}"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--apply", action="store_true")
    args = ap.parse_args()

    db = sqlite3.connect(DB_PATH)
    known = {r[0] for r in db.execute("SELECT id FROM anniversaries")}
    brands = {r[0] for r in db.execute("SELECT id FROM brands")}
    # 同じブランドの同じ日に同じ種別が既にあるなら重ねない。
    existing_keys = {(r[0], r[1], r[2])
                     for r in db.execute("SELECT brand_id, date, kind FROM anniversaries")}

    rows, skipped = [], []
    for brand, date, label, kind, *_source in ENTRIES:
        if brand not in brands:
            skipped.append(f"{label} (brand '{brand}' が存在しない)")
            continue
        key = (brand, date, kind)
        if key in existing_keys:
            skipped.append(f"{label} (同ブランド・同日・同種別が既にある)")
            continue
        rid = entry_id(brand, date, kind)
        if rid in known:
            skipped.append(f"{label} (id 重複)")
            continue
        rows.append((rid, brand, label, date, kind, 0))

    print(f"追加: {len(rows)} 件 / 見送り: {len(skipped)} 件")
    for r in rows:
        print(f"  {r[3]}  [{r[1]}] {r[2]}  ({r[4]})")
    for s in skipped:
        print(f"  - {s}")

    if not args.apply:
        print("\n(--apply で master.sqlite に反映する)")
        return

    db.executemany(
        "INSERT INTO anniversaries (id, brand_id, label, date, kind, sort_order)"
        " VALUES (?,?,?,?,?,?)", rows)
    print(f"\nanniversaries に {len(rows)} 件追加した"
          f" (合計 {db.execute('SELECT count(*) FROM anniversaries').fetchone()[0]} 件)")

    # 正本は書く前に一時 DB で外部キーを検査してから書き出す (壊れていれば書かない)。
    masterdb.write_master_sql(db, DUMP_PATH)


if __name__ == "__main__":
    main()
