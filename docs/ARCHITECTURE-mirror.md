# CloudKit の公開データをマスタと同じ構造の SQLite に写す (外部の利用者向け)

> 状態: **実装済み** (2026-10-11)。部品は `web/wasm/imas-sync-wasm/` (wasm-bindgen の薄いラッパ) と `imas-core/src/domain/mirror_*.rs` (規則の実体)。
> アプリ (iOS / Android) の同期の仕組みは [`ARCHITECTURE.md`](ARCHITECTURE.md#データの所在同期マイグレーション-ios--android-共通の思想) と [`ARCHITECTURE-android.md`](ARCHITECTURE-android.md)。
> 項目の束 (`build_idol_facts`) の定義は [`ARCHITECTURE-facets.md`](ARCHITECTURE-facets.md)。

ImasLiveDB のマスタの正は **CloudKit の公開データベース** (コンテナ `iCloud.com.fugaif.ImasLiveDB`・環境 `production`)。誰でも読める (読み取り専用の API トークンは CloudKit Dashboard → Tokens → API Tokens で自分で発行する)。
アプリは端末の DB に写す。**同じ規則で、サーバーや自分のツールの SQLite (Cloudflare D1 など) にも写せる**ようにしたのがこの部品。

## 立ち位置: 通信と書き込みだけが呼ぶ側、規則は imas-core

| やること | 担当 |
|---|---|
| CloudKit Web Services の HTTP (`records/query`)・トークン・SQL の実行・定期実行の刻み方 | **呼ぶ側** (TS でも何でも) |
| 写す表の DDL・ステップの順序 | imas-core `domain/mirror_schema.rs` |
| レコード → 行の変換・書く SQL・削除する鍵・孤児の鍵 | imas-core `domain/mirror_ingest.rs` (変換規則は `ck_record_mapping.rs` を**アプリと共有**) |
| 差分同期の判定 (full / incremental・起点・巻き戻し・soft delete・埋め直し・孤児・完了・失敗の分類) | imas-core `sync_planning.rs` / `sync_decisions.rs` (アプリと共有) + `mirror_policy.rs` (写す側の運用) |
| 写した表 → Snapshot → アイドルの項目の束 | imas-core `domain/mirror_load.rs` → `idol_facets::build_idol_facts` |

`web/wasm/imas-sync-wasm` はこれらを JSON で呼ぶだけ。**規則は 1 行も書いていない** (`imas-query-wasm` と同じ流儀)。JS の型は ts-rs が `web/wasm/imas-sync-wasm/types/` に出す。

## 使い方

### 0. 取り込む

```bash
cd web/wasm/imas-sync-wasm
wasm-pack build --release --target web --out-dir pkg      # pkg/imas_sync_wasm{.js,_bg.wasm,.d.ts}
```

`--target web` の glue は Workers では `initSync({ module })` で初期化する (wrangler が `.wasm` の import を `WebAssembly.Module` にする)。
大きさは **1.68 MB (gzip 425 KB)**。Cloudflare Workers の有料プランの上限 (圧縮後 10 MB) に余裕で収まる。型 (`types/*.ts`) は一緒にコピーする。

### 1. 表を作る

```ts
const steps = JSON.parse(mirror_steps("[]"));               // MirrorStep[]: FK 依存順。写す型を絞るなら ["Idol","Song",…] を渡す
const ddl = mirror_ddl(JSON.stringify(steps.map(s => s.recordType)));   // CREATE TABLE を ";" 区切りで
```

- 表と列は**同梱マスタと同じ**。違いは **FOREIGN KEY と複合 UNIQUE を外している**こと (CloudKit にある通りに写す側なので、親が後から届いても、同じ位置のセトリが一時的に重なっても書き込みを失敗させない。親の無い行は読み出し側が孤児として読み飛ばす)。
- アプリが取り込まない `IdolFacet` / `IdolHairstyle` (`idol_facets` / `idol_hairstyles`) も写せる。`brands.icon_url`・`idols.voice_actors` はマスタの DDL に無いがアプリの移行や旧データの都合で持つ列で、`mirror_ddl` が足す。
- 索引は既定では付けない (D1 は書き込みの数で課金される)。読み方が決まっていれば `mirror_index_ddl(types)` (一意でない索引だけ)。
- 実行の状態の表は呼ぶ側が持つ。孤児掃除のための **`mirror_seen`** (`MIRROR_SEEN_DDL`) は DDL が imas-core にある: `(record_type, run_id, id)`。

### 2. 1 ページを取り込む

```ts
const body = await (await fetch(url, { method: "POST", body: JSON.stringify({
  resultsLimit: 200,
  query: { recordType, filterBy: [{ fieldName: "modifiedAt", comparator: "GREATER_THAN", fieldValue: { value: sinceMs, type: "TIMESTAMP" } }],
           sortBy: [{ fieldName: "modifiedAt", ascending: true }] },
  ...(marker ? { continuationMarker: marker } : {}),
}) })).text();

const page: MirrorPage = JSON.parse(ingest_page(recordType, body, Date.now()));
await db.batch([
  page.rows.length && db.prepare(page.upsertSql).bind(JSON.stringify(page.rows)),
  page.deletedKeys.length && db.prepare(page.deleteSql).bind(JSON.stringify(page.deletedKeys)),
  // 全件を取り直すステップだけ: 見えた鍵を溜める
  fromEpoch && page.seenIds.length && db.prepare(step.seenSql).bind(recordType, runId, JSON.stringify(page.seenIds)),
].filter(Boolean));
```

- **応答の本文を JSON のまま渡す。** `fields` を `value` だけに平坦化すると TIMESTAMP / INT64 / DOUBLE が区別できなくなり、soft delete (`deletedAt`) が伝わらない。`serverErrorCode` のレコードを除くのと `continuationMarker` の取り出しも `ingest_page` がやる。
- `upsertSql` は `INSERT … SELECT … FROM json_each(?1) … ON CONFLICT … DO UPDATE … WHERE <変わった列がある>`。行の配列を **束縛 1 つ**で渡すので、D1 の「1 文の束縛は 100 個まで」に当たらない。**同じ内容の行は書かない**ので `meta.changes` が「実際に変わった行の数」になり、書き込みの課金も変更分だけ。
- 削除の鍵は**行から取る** (soft delete したレコードにも `fields` は残る)。アプリのように `recordName` を前から割ると、先頭側の主キーに `-` を含む行 (`unit_id = "3-9days"`) の削除が当たらない。変換できなかったときだけ `recordName` を割り、それも割れなければ `unresolvedDeleted` に載る。
- 色は `#RRGGBB` にする (アプリの変換は iOS の `HexColor` に合わせて `#` を剥がした形を返すが、マスタと読み手は `#` つきを前提にする)。

### 3. 差分同期の流れ

判定は全部 imas-core の関数。時刻は**秒 (f64)** (`ms / 1000` で渡し、`Math.round(sec * 1000)` で戻す。ミリ秒のまま渡すと巻き戻し幅 1ms が 1000 倍ずれる)。

```
startup_plan(状態)        → full / incremental と起点         (保留中のフル → 表が空 → 定期フルの期限 → 起点なし → 差分)
incremental_since(起点)   → 差分の実行の起点 (2 時間遡る)       写す側の規則 (下の表)
run_start_plan(…)         → 実行の起点 (フルは取り切るまで始めた時刻を引き継ぐ)
for step of steps:        // FK 依存順
  step_start(型, …, backfilled) → 取得の起点 / skip / started_from_epoch   (一度も取り切っていない型は差分でも epoch から)
  loop: ページを取る → ingest_page → 書く
        chunk_progress(前の最大, page.modifiedAtMillis) → 区間の最大・チェックポイント
        next_chunk_action(続きがあるか, 実際に変わった行の数, 区間の最大) → continue / restart_from (最大 − 1ms から張り直し) / finish
  取り切ったら: should_delete_orphans(型, started_from_epoch) なら孤児掃除 (下)
completion_plan(…)        → last_sync を残すか・保留中のフルを消すか・変わったか
```

- `next_chunk_action` の `added_since_restart` は **実際に変わった行の数** (D1 の `changes`) を渡す。取得件数を渡すと、境界の張り直しで取り直した同じレコードが毎回新規に数えられ、ステップが終わらない。
- 失敗は `classify_fetch_error(信号, 続きのページか)`: `unknown_record_type` (本番にまだ型が無い = そのステップを飛ばして続ける。**取り切った扱いにしない**) / `retryable` (通信断・429・5xx) / `auth` (401・403) / `fatal`。続きのページの「型が無い」はマーカーの失効なので `fatal` (チェックポイントの手前から張り直す)。
- 1 回の実行を何度かの起動に分けるなら (Workers のサブリクエスト上限など)、型ごとの進み具合 (起点・マーカー・区間の最大・チェックポイント・取り切った実行) を保存して、次の起動が続きから読む。

### 4. 孤児 (CloudKit で物理削除された行) を消す

soft delete (`deletedAt`) は差分で伝わるが、**物理削除は通知が来ない**。全件を取り直した完走 (`started_from_epoch`) でだけ、見えなかった行を消す。

1. 取り直しの各ページで `page.seenIds` を `step.seenSql` で `mirror_seen` に溜める (単一主キーは値、複合主キーは値の JSON 配列)。
2. 完走したら `step.orphanProbeSql` (`seen` / `total` / `orphans`) を引く。`seen = 0` なら消さない (取得 0 件は通信異常でも起きる)。
3. **`orphan_delete_allowed(orphans, total)` を通す**: 1 回の掃除で消せるのは型の行数の 5% (小さい型は 3 件) まで。超えるなら取りこぼし・壊れた応答を疑って**消さずに知らせる**。
4. 通れば `step.orphanDeleteSql` を流し、`mirror_seen` の行を捨てる。

アプリは単一主キーの表だけ掃除するが、**写しは複合主キーの表も掃除する**: セトリの出演者 (`setlist_performers`) などの行は実際に CloudKit で物理削除される (2026-10-11 の実測で 1,150 行)。

### 5. アイドルの項目の束を組む

```ts
const facts = new IdolFacts();
for (const table of loadable_tables())                    // brands idols idol_facets idol_hairstyles units unit_members songs song_artists events shows show_cast setlist_items setlist_performers (+ idol_brands idol_voice_actors)
  for (let last = 0; ; ) {                                // rowid 順に 8000 行ずつ
    const rows = (await db.prepare(`SELECT rowid AS _rowid, * FROM ${table} WHERE rowid > ?1 ORDER BY rowid LIMIT 8000`).bind(last).all()).results;
    if (rows.length) facts.add_rows(table, JSON.stringify(rows));
    if (rows.length < 8000) break; last = rows.at(-1)._rowid;
  }
facts.finish();                                           // Snapshot を組む (索引)
for (const id of facts.idol_ids()) doc[id] = JSON.parse(facts.facts(id, tagsJson));   // /facts と同じ文書。話し方は imas-core が持つ公式データ
facts.free();
```

**全アイドルを 1 つの Snapshot から組む**方式にした (1 人分だけ組む API は作っていない)。`SELECT` は 1 表 1 周なので D1 の呼び出しは「いちばん大きい表の行数 / 8000」周で済む (実測 120k 行で 8 周)。
アイドルごとに関係する行を引く方式は、結合の順と索引を固定しないと D1 (統計を持たない) が全表を舐める計画を選び、規則 (誰が原唱者か・ソロ曲の数え方・共演者) を SQL にもう一度書くことになる。Snapshot に渡せば `build_idol_facts` が**アプリ・Web・MCP と同じ関数**でそのまま動く。

実測 (実データ 115,827 行・394 人。Node で wasm を回した CPU): 行の追加 175 ms + Snapshot 構築 123 ms + 全員分の項目の束 241 ms = **約 540 ms**、wasm のメモリ 25 MB。

行は渡した時点で型つきの行に直して持つ (JSON のまま溜めない)ので、ページに分ければ JS 側のメモリは 1 ページ分。**親の無い行 (存在しないイベントの公演など) は読み飛ばす** (`outbound::sqlite_loader` と同じ)。

## アプリの同期との違い (写す側の規則)

アプリの関数は変えず、足りない分を `mirror_policy.rs` に足した。

| 規則 | アプリ | 写す側 |
|---|---|---|
| 差分の起点 | 前回の実行を始めた時刻ちょうど | **2 時間遡る** (`incremental_since`)。ImasLiveDB の送り込みと実行が重なると、始めた時刻より前の `modifiedAt` が後から見える。重なって読み直した行は内容が同じなら書かない |
| 孤児の掃除 | 単一主キーの表だけ。`valid_ids` が空でなければ全部消す | **複合主キーも**。**1 回で型の行数の 5% まで** (`orphan_delete_allowed`) |
| 型の埋め直し | 端末の表が空のときだけ (アプリ固有) | **一度も取り切っていない型は epoch から** (`step_start` の `backfilled`)。本番に未昇格の型 (`IdolFacet` など) が、できたあとも取りこぼさない |
| 取得の失敗 | iOS の `CKError` だけ再試行を知る | **HTTP の失敗も分類**。続きのページの NOT_FOUND はマーカーの失効 (`classify_fetch_error`) |
| 削除の鍵 | `recordName` を前から割る | **行から取る** |
| 色 | `#` なし | `#RRGGBB` |
| 声優 | 読まない (履歴の表が正。履歴の型は CloudKit にまだ無い) | 廃止予定の `Idol.voiceActors` を `idols.voice_actors` に持ち、**履歴の行が無いアイドルの現在の声優**にする |
| 制約 | FK・UNIQUE あり | 外す |

## テスト

- `cd imas-core && cargo test --locked --lib mirror_` — DDL がマスタと同じ列を持つこと、**全 24 型を実物の SQLite に流して** 初回で 1 行・同じ内容で 0 行・変更で 1 行・soft delete で消えること、孤児掃除 (単一・複合)、削除の鍵、色、`mirror_load` が同じ DB から作った Snapshot で**全アイドルの項目の束が `sqlite_loader` 経由と一致すること**。
- `cargo test --locked --manifest-path web/wasm/imas-sync-wasm/Cargo.toml` — JSON の出し入れ (時刻の単位・タグつきの enum・DDL・ページ・項目の束)。
- CI: `core-guard` が上の 2 つと wasm32 のビルドと、型 (`types/`) が再生成して差が出ないことを見る。

## 既知の限界

- 写すレコードタイプは imas-core の `mirror_steps`。**声優の履歴 (`IdolVoiceActor`)・スタッフ・記念日・曲の対応 (`song_units`) は CloudKit に型が無い**ので写せない。
- Snapshot の構築 (約 120 ms / 12 万行) と項目の束は、行を全部渡してからでないと組めない。差分で変わったアイドルだけ組み直す API は無い (全員で約 0.5 秒なので、変わったときに全員を組み直す)。
- `mirror_load` が受ける表は `LOADABLE_TABLES` (項目の束と一覧・検索が読む分)。会場・衣装・チケットなどを Snapshot にしたいなら足す (`outbound::sqlite_loader` と同じ並び・孤児の扱いで、同じ DB から両方で組んだものが一致するテストを足す)。
