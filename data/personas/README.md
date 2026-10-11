# data/personas

キャラとのトーク・タイムライン (アプリ内のオプトイン機能、試作) で使う「話し方」のデータ。
imas-core (`agent::tools::speech`) がアプリに同梱して読む。DB や CloudKit には入れていない。

- `<brand>.json` — 1 人ずつ出典を確かめた話し方。`idol_id` / `name` / `first_person` /
  `producer_call` / `politeness` / `endings` / `catchphrases` / `tone_notes` / `sources` の配列。
  一人称・呼び方は文字列 / 文字列の配列 / `{form, scene}` の配列のどれでもよい。
  元の資料と呼称表で食い違う場合は、両方を配列に並べる (先頭が元の資料の値)。
  「○○」はプレイヤーが付ける名前 (例「○○さん」=プレイヤー名+さん)。
  ブランドを足したら `imas-core/src/agent/tools/speech.rs` の `PERSONA_FILES` に 1 行足す。
- `call_names.json` — 誰が誰をどう呼ぶか (`from` / `to` (`producer` はプロデューサー、`from` と同じ id は
  一人称) / `called` / `source`)。`source` は行の出どころの URL で、2 種類ある。
  - [im@sparql](https://github.com/crssnky/imasparql) の呼称データ (imas:CallName)
    (`source` = `https://sparql.crssnky.xyz/spql/imas/query`)。
    `tools/fetch_call_names_from_sparql.py` でこちらのアイドル id に突き合わせたもの。
    im@sparql は MIT License (`LICENSE-imasparql.txt`)。
  - ニコニコ大百科の呼称表 (765・シンデレラガールズ・ミリオンライブ！・SideM・シャイニーカラーズ・
    学園アイドルマスターの 6 ページ。`source` = 各ページの URL `https://dic.nicovideo.jp/t/a/…:呼称表`)。
    `tools/fetch_call_names_from_nicodic.py` で表を読んで突き合わせたもの。
    名前が突き合わせられなかった行 (表記ゆれ・マスタにいない人・表の並びがずれている箇所) は
    `data/research/personas_20261012/unmatched_call_names.csv` に出している。
  同じ `from`/`to` に両方の出どころがある場合は両方の行を残す (読む側 `speech.rs` は呼び方を重複なく束ねる)。
  どちらの取得ツールも、相手の出どころの行は消さずに自分の行だけ作り直す。

セリフ (コミュの台本) は書き写しなので入れない。
