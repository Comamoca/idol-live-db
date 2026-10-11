//! CloudKit の公開データを「マスタと同じ構造の SQLite」に写すときの、表の定義と同期の手順 (ステップ)。
//!
//! アプリ (iOS / Android) は端末の DB にマスタを写す。同じ規則で **サーバーや外部の利用者の SQLite
//! (Cloudflare D1 など) にも写せる**ようにするのが `mirror_*` の 4 つのモジュール:
//!
//! - `mirror_schema` (ここ): 写す表の DDL と、同期の手順 (FK 依存順のステップ)
//! - [`crate::domain::mirror_ingest`]: CloudKit のレコード JSON → 表の行 / 削除する鍵 / 孤児判定の id
//! - [`crate::domain::mirror_policy`]: アプリの同期に無い、写す側の運用の規則 (遡り・埋め直し・孤児の歯止め)
//! - [`crate::domain::mirror_load`]: 写した表の行 → [`crate::domain::snapshot::Snapshot`] (読み出し側)
//!
//! 通信 (HTTP・トークン・ページング) と書き込み (SQL の実行) は含まない。呼ぶ側が持つ。
//!
//! ## 表は「マスタと同じ」ただし制約は外す
//!
//! 表と列は同梱マスタ ([`crate::domain::schema_ddl::MASTER_SCHEMA_SQL`]) と同じ。ただし **FOREIGN KEY と
//! 複合 UNIQUE は外す**。写し先は CloudKit にある通りの内容を持つ側で、親が後から届く (ステップは FK 依存順だが
//! 差分では前後する) ことも、`setlist_items` の (show_id, position) が一時的に重なることも起きる。
//! 制約違反で 1 ページ分の書き込みごと失敗して同期が止まるより、重なりは写して読む側 (Snapshot の構築) が
//! 孤児として読み飛ばすほうが被害が小さい。索引は既定では作らない ([`mirror_index_ddl`] で別に取る)。

use crate::domain::schema_ddl::MASTER_SCHEMA_SQL;
use crate::domain::sync_planning::{steps_for, table_info, SyncTableInfo};

/// アイドルの項目・髪型の表の DDL (同梱マスタには、CloudKit に昇格されるまで入らない)。
pub const FACETS_SCHEMA_SQL: &str = include_str!("facets_schema.sql");

/// `song_videos` (`SongVideo`) は同梱マスタに無く、アプリが自分の移行で作る表。写す側が使えるよう列を揃える
/// ([`crate::domain::ck_record_mapping::CkSongVideoRow`] と同じ)。
const SONG_VIDEOS_DDL: &str = "CREATE TABLE song_videos (
  id TEXT PRIMARY KEY NOT NULL,
  song_id TEXT NOT NULL,
  youtube_url TEXT NOT NULL,
  video_title TEXT,
  note TEXT,
  created_at TEXT NOT NULL,
  author_display_name TEXT
)";

/// 同梱マスタの DDL に無いが、CloudKit のレコードが持ち、アプリが自分の移行で足している列。
/// 写す側にはこれが無いと upsert が落ちる (`CkBrandRow::icon_url`)。`idols.voice_actors` は廃止予定の声優の列で、アプリの変換は読まないが写す側は持つ
/// ([`crate::domain::ck_record_mapping::legacy_voice_actors`])。テストが「行の全列が表にある」ことを固定する。
const MIRROR_EXTRA_COLUMNS: &[(&str, &str)] = &[("brands", "icon_url TEXT"), ("idols", "voice_actors TEXT")];

/// 同期 1 ステップ = 1 レコードタイプ。FK 依存順 (親が先)。
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "web-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../web/wasm/imas-sync-wasm/types/")
)]
pub struct MirrorStep {
    /// CloudKit のレコードタイプ名 (`Idol` / `Song` …)。
    pub record_type: String,
    /// 進捗・ログ用のラベル。
    pub display_name: String,
    /// 写し先の表。
    pub table: String,
    /// 主キーの列 (複合主キーは複数)。
    pub pk_columns: Vec<String>,
    /// 全件を取り直した完走で孤児掃除をしてよい型か。**写しは全部の型で行う** (アプリは単一主キーの型だけ)。
    /// 複合主キーの行も CloudKit で物理削除される (セトリの出演者など。tombstone が来ないので、全件を取り直した完走でしか消せない)。
    pub orphan_cleanup: bool,
    /// 全件を取り直すステップで、ページごとに「CloudKit に在った行」の鍵を [`MIRROR_SEEN_TABLE`] に足す SQL。
    /// `?1` = レコードタイプ、`?2` = 実行の id、`?3` = `MirrorIngest::seen_ids` の JSON 配列。
    pub seen_sql: String,
    /// 孤児の数・見えた鍵の数・表の行数を 1 行で返す SQL (`seen` / `orphans` / `total`)。`?1` = レコードタイプ、`?2` = 実行の id。
    pub orphan_probe_sql: String,
    /// 孤児 (見えた鍵に無い行) を消す SQL。同じ束縛。**呼ぶ前に** [`crate::domain::mirror_policy::orphan_delete_allowed`] を通す。
    pub orphan_delete_sql: String,
}

/// 「CloudKit に在った行」の鍵を実行ごとに溜める表の名前。DDL は [`MIRROR_SEEN_DDL`]。
pub const MIRROR_SEEN_TABLE: &str = "mirror_seen";

/// [`MIRROR_SEEN_TABLE`] の DDL。`id` は単一主キーなら主キーの値、複合主キーなら値の JSON 配列 (`["a","b"]`)。
/// 状態の表なので [`mirror_ddl`] には含めない (呼ぶ側の migration に入れる)。実行が済んだら `run_id` 以下の行を捨てる。
pub const MIRROR_SEEN_DDL: &str = "CREATE TABLE mirror_seen (
  record_type TEXT NOT NULL,
  run_id INTEGER NOT NULL,
  id TEXT NOT NULL,
  PRIMARY KEY (record_type, run_id, id)
) WITHOUT ROWID";

/// 写し先の表と主キー。アプリの同期の対応表 ([`table_info`]) に、アプリが取り込まない型
/// (`IdolFacet` / `IdolHairstyle`) を足したもの。未知のレコードタイプは `None`。
pub fn mirror_table_info(record_type: &str) -> Option<SyncTableInfo> {
    match record_type {
        "IdolFacet" => Some(SyncTableInfo { table: "idol_facets".into(), pk_columns: vec!["id".into()] }),
        "IdolHairstyle" => Some(SyncTableInfo { table: "idol_hairstyles".into(), pk_columns: vec!["id".into()] }),
        other => table_info(other),
    }
}

/// 写すステップを FK 依存順に返す。`available` が空なら全部、そうでなければその型だけ (並びは常にこちらの定義が正)。
///
/// アプリの全ステップ ([`crate::domain::sync_planning::all_steps`]) に、アイドルの項目と髪型
/// (`IdolFacet` / `IdolHairstyle`。`idols` にだけ依存する) を `Idol` の直後に足す。
pub fn mirror_steps(available: &[String]) -> Vec<MirrorStep> {
    let mut out = Vec::new();
    for step in steps_for(&[]) {
        let is_idol = step.record_type == "Idol";
        out.push(step_of(&step.record_type, &step.display_name));
        if is_idol {
            out.push(step_of("IdolFacet", "アイドルの項目"));
            out.push(step_of("IdolHairstyle", "アイドルの髪型"));
        }
    }
    out.retain(|s| available.is_empty() || available.iter().any(|a| *a == s.record_type));
    out
}

fn step_of(record_type: &str, display_name: &str) -> MirrorStep {
    // 呼ぶのは steps_for と EXTRA だけなので必ず引ける。
    let info = mirror_table_info(record_type).expect("ステップの型は表を持つ");
    let (seen_sql, orphan_probe_sql, orphan_delete_sql) = orphan_sql(&info.table, &info.pk_columns);
    MirrorStep {
        record_type: record_type.to_string(),
        display_name: display_name.to_string(),
        orphan_cleanup: true,
        table: info.table,
        pk_columns: info.pk_columns,
        seen_sql,
        orphan_probe_sql,
        orphan_delete_sql,
    }
}

pub(crate) fn quote(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

/// 孤児掃除の SQL 3 つ (見えた鍵を足す・確認・削除)。鍵は単一主キーなら値そのもの、複合なら JSON 配列。
fn orphan_sql(table: &str, pk_columns: &[String]) -> (String, String, String) {
    let t = quote(table);
    let cols = pk_columns.iter().map(|c| quote(c)).collect::<Vec<_>>().join(", ");
    let seen_keys = if pk_columns.len() == 1 {
        "id".to_string()
    } else {
        (0..pk_columns.len()).map(|i| format!("json_extract(id, '$[{i}]')")).collect::<Vec<_>>().join(", ")
    };
    let not_seen = format!(
        "({cols}) NOT IN (SELECT {seen_keys} FROM {MIRROR_SEEN_TABLE} WHERE record_type = ?1 AND run_id = ?2)"
    );
    let seen_sql = format!("INSERT OR IGNORE INTO {MIRROR_SEEN_TABLE} (record_type, run_id, id) SELECT ?1, ?2, value FROM json_each(?3)");
    let probe = format!(
        "SELECT (SELECT COUNT(*) FROM {MIRROR_SEEN_TABLE} WHERE record_type = ?1 AND run_id = ?2) AS seen, (SELECT COUNT(*) FROM {t}) AS total, (SELECT COUNT(*) FROM {t} WHERE {not_seen}) AS orphans"
    );
    let delete = format!("DELETE FROM {t} WHERE {not_seen}");
    (seen_sql, probe, delete)
}

/// 指定したレコードタイプの CREATE TABLE を、制約 (FOREIGN KEY・複合 UNIQUE) を外して `;` 区切りで返す。
/// 並びは `record_types` の順 (FK 依存順の [`mirror_steps`] をそのまま渡す)。未知の型・重複は飛ばす。
/// 流す先は SQLite ならどれでもよい (Cloudflare D1 は 1 文ずつ `exec` / `prepare` に渡す)。
pub fn mirror_ddl(record_types: &[String]) -> String {
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<String> = Vec::new();
    for rt in record_types {
        let Some(info) = mirror_table_info(rt) else { continue };
        if seen.contains(&info.table) {
            continue;
        }
        if let Some(create) = create_table_sql(&info.table) {
            out.push(create);
            seen.push(info.table);
        }
    }
    out.join(";\n") + if out.is_empty() { "" } else { ";\n" }
}

/// 指定したレコードタイプの表につく**一意でない**索引の CREATE INDEX (マスタと同じ名前・列)。
/// 一意索引は写しで衝突させないため含めない。書き込みのたびに索引も更新されるので、読み方が決まっていなければ付けなくてよい。
pub fn mirror_index_ddl(record_types: &[String]) -> String {
    let tables: Vec<String> = record_types.iter().filter_map(|rt| mirror_table_info(rt)).map(|i| i.table).collect();
    let mut out = String::new();
    for sql in schema_statements() {
        let upper = sql.to_uppercase();
        if !upper.starts_with("CREATE INDEX ") {
            continue;
        }
        let Some(on) = index_table(&sql) else { continue };
        if tables.contains(&on) {
            out.push_str(&sql);
            out.push_str(";\n");
        }
    }
    out
}

/// 表の CREATE 文 (制約を外した形)。同梱マスタ → アイドルの項目の表 → `song_videos` の順に探す。
fn create_table_sql(table: &str) -> Option<String> {
    let found = schema_statements().into_iter().find(|s| table_of_create(s).as_deref() == Some(table));
    match found {
        Some(sql) => Some(strip_constraints(&sql, table)),
        None if table == "song_videos" => Some(SONG_VIDEOS_DDL.to_string()),
        None => None,
    }
}

/// 同梱マスタとアイドルの項目の DDL を 1 文ずつに割る (コメント行は捨てる)。
fn schema_statements() -> Vec<String> {
    [MASTER_SCHEMA_SQL, FACETS_SCHEMA_SQL]
        .into_iter()
        .flat_map(|sql| {
            let body: String = sql
                .lines()
                .filter(|l| !l.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n");
            body.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect::<Vec<_>>()
        })
        .collect()
}

fn table_of_create(sql: &str) -> Option<String> {
    let rest = sql.strip_prefix("CREATE TABLE ")?;
    let name = rest.split(['(', ' ', '\n']).next()?.trim().trim_matches('"');
    (!name.is_empty()).then(|| name.to_string())
}

/// `CREATE INDEX idx ON table(col…)` の table。
fn index_table(sql: &str) -> Option<String> {
    let after = sql.split(" ON ").nth(1)?;
    let name = after.split(['(', ' ']).next()?.trim().trim_matches('"');
    (!name.is_empty()).then(|| name.to_string())
}

/// CREATE TABLE から FOREIGN KEY・UNIQUE(…) の表制約と、列の `REFERENCES …` を外す。
fn strip_constraints(sql: &str, table: &str) -> String {
    let (Some(open), Some(close)) = (sql.find('('), sql.rfind(')')) else { return sql.to_string() };
    let name = table_of_create(sql).unwrap_or_default();
    let mut items: Vec<String> = Vec::new();
    for item in split_top_level(&sql[open + 1..close]) {
        let item = item.trim();
        let upper = item.to_uppercase();
        if upper.starts_with("FOREIGN KEY") || upper.starts_with("UNIQUE") || upper.starts_with("CONSTRAINT") {
            continue;
        }
        items.push(strip_references(item));
    }
    items.extend(MIRROR_EXTRA_COLUMNS.iter().filter(|(t, _)| *t == table).map(|(_, def)| def.to_string()));
    format!("CREATE TABLE {name} (\n  {}\n)", items.join(",\n  "))
}

/// 括弧の外のカンマで割る。
fn split_top_level(body: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in body.chars() {
        match c {
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        parts.push(cur);
    }
    parts
}

/// 列の定義から ` REFERENCES t(c) [ON DELETE …]` を外す。
fn strip_references(item: &str) -> String {
    let upper = item.to_uppercase();
    match upper.find(" REFERENCES ") {
        Some(at) => item[..at].trim_end().to_string(),
        None => item.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::collections::BTreeMap;

    fn all_types() -> Vec<String> {
        mirror_steps(&[]).into_iter().map(|s| s.record_type).collect()
    }

    /// 「表名 → 列の並び (名前・型・NOT NULL・既定値・主キー順)」。
    fn columns_of(conn: &Connection) -> BTreeMap<String, Vec<String>> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
            .unwrap();
        let names: Vec<String> = stmt.query_map([], |r| r.get(0)).unwrap().filter_map(Result::ok).collect();
        names
            .into_iter()
            .map(|t| {
                let mut s = conn.prepare(&format!("PRAGMA table_info(\"{t}\")")).unwrap();
                let cols: Vec<String> = s
                    .query_map([], |r| {
                        Ok(format!(
                            "{}|{}|{}|{:?}|{}",
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, i64>(3)?,
                            r.get::<_, Option<String>>(4)?,
                            r.get::<_, i64>(5)?
                        ))
                    })
                    .unwrap()
                    .filter_map(Result::ok)
                    .collect();
                (t, cols)
            })
            .collect()
    }

    #[test]
    fn steps_follow_fk_order_and_put_the_facet_types_right_after_idol() {
        let types = all_types();
        let pos = |t: &str| types.iter().position(|x| x == t).unwrap();
        assert_eq!(pos("IdolFacet"), pos("Idol") + 1);
        assert_eq!(pos("IdolHairstyle"), pos("Idol") + 2);
        assert!(pos("Brand") < pos("Idol") && pos("Idol") < pos("Show") && pos("Show") < pos("SetlistItem"));
        // 取り込む型の全部 + 項目と髪型
        assert_eq!(types.len(), crate::domain::ck_record_mapping::INGESTED_RECORD_TYPES.len() + 2);
    }

    #[test]
    fn available_filters_but_keeps_our_order() {
        let want = vec!["Song".to_string(), "IdolHairstyle".to_string(), "Brand".to_string()];
        let got: Vec<String> = mirror_steps(&want).into_iter().map(|s| s.record_type).collect();
        assert_eq!(got, ["Brand", "IdolHairstyle", "Song"]);
    }

    #[test]
    fn every_step_can_sweep_orphans_unlike_the_apps() {
        // 複合主キーの行も CloudKit で物理削除される (セトリの出演者など)。アプリは単一主キーだけ。
        assert!(mirror_steps(&[]).iter().all(|s| s.orphan_cleanup));
        assert!(mirror_steps(&[]).iter().any(|s| s.pk_columns.len() > 1));
    }

    /// 全部の型の DDL が SQLite で通り、列がマスタ (と項目の表) の定義と一致する。制約は外れている。
    #[test]
    fn ddl_runs_and_keeps_every_column_of_the_master() {
        let mirror = Connection::open_in_memory().unwrap();
        mirror.execute_batch(&mirror_ddl(&all_types())).unwrap();
        // 外部キーがあれば、親の無い行を入れた時点で落ちる。入っても構わない。
        mirror.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        mirror.execute("INSERT INTO shows (id, event_id, name, date, sort_order) VALUES ('s','no-such-event','n','2026-01-01',0)", []).unwrap();
        mirror
            .execute("INSERT INTO setlist_items (id, show_id, song_id, position) VALUES ('a','s','x',1), ('b','s','x',1)", [])
            .expect("(show_id, position) の重複を許す");

        let master = Connection::open_in_memory().unwrap();
        master.execute_batch(MASTER_SCHEMA_SQL).unwrap();
        master.execute_batch(&FACETS_SCHEMA_SQL.lines().filter(|l| !l.trim_start().starts_with("--")).collect::<Vec<_>>().join("\n")).unwrap();
        let (m, mi) = (columns_of(&master), columns_of(&mirror));
        for step in mirror_steps(&[]) {
            if step.table == "song_videos" {
                continue; // マスタに無い表 (列は CkSongVideoRow と下のテストで揃える)
            }
            let mut expected = m.get(&step.table).cloned().unwrap();
            for (t, def) in MIRROR_EXTRA_COLUMNS {
                if *t == step.table {
                    let name = def.split(' ').next().unwrap();
                    expected.push(format!("{name}|TEXT|0|None|0"));
                }
            }
            assert_eq!(mi.get(&step.table), Some(&expected), "{} の列がマスタと違う", step.table);
        }
        assert!(mi.contains_key("song_videos"));
    }

    #[test]
    fn index_ddl_has_no_unique_index_and_runs() {
        let types = all_types();
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&mirror_ddl(&types)).unwrap();
        let idx = mirror_index_ddl(&types);
        assert!(!idx.to_uppercase().contains("UNIQUE"));
        assert!(idx.contains("idx_setlist_performers_idol"));
        conn.execute_batch(&idx).unwrap();
    }

    #[test]
    fn ddl_for_a_subset_has_only_those_tables() {
        let ddl = mirror_ddl(&["Brand".to_string(), "Idol".to_string(), "Nothing".to_string(), "Idol".to_string()]);
        assert_eq!(ddl.matches("CREATE TABLE").count(), 2);
        assert!(ddl.contains("CREATE TABLE brands") && ddl.contains("CREATE TABLE idols"));
        assert!(!ddl.contains("REFERENCES") && !ddl.contains("FOREIGN"));
    }
}
