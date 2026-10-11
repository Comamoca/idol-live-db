//! CloudKit のレコード JSON → 写し先の表の行。
//!
//! アプリの取り込み ([`crate::domain::ck_record_mapping`]) と**同じ変換規則**で、行を「列名の並び + 値の並び」の
//! 形にして返す。呼ぶ側は返った SQL に JSON を 1 つ束縛して流すだけで、列の対応も、削除する鍵の組み立ても、
//! 孤児掃除に使う id の集計も持たない。
//!
//! # 1 ページの使い方
//!
//! 1. CloudKit Web Services の `records/query` を 1 ページ取る (`serverErrorCode` を持つレコードは除く)。
//! 2. `records` の各要素を JSON 文字列のまま [`ingest`] に渡す (**`fields` を `value` だけに平坦化しない**。
//!    TIMESTAMP / INT64 / DOUBLE が区別できなくなり、soft delete が伝搬しない)。
//! 3. `rows` が空でなければ `upsert_sql` に `JSON.stringify(rows)` を束縛して流す。
//! 4. `deleted_keys` が空でなければ `delete_sql` に `JSON.stringify(deleted_keys)` を束縛して流す。
//! 5. 全件を取り直したステップの完走時、`seen_ids` を集めておいた集合に無い id を孤児として消す
//!    ([`crate::domain::mirror_policy::orphan_delete_allowed`] の歯止めを通す)。
//!
//! # アプリの取り込みとの違い
//!
//! - **削除の鍵は行から取る。** アプリは `recordName` を `"{table}-{pk1}-{pk2}"` として前から割る
//!   ([`crate::domain::sync_planning::parse_composite_record_name`]) ので、先頭側の主キーに `-` を含む行
//!   (`unit_id = "3-9days"` など) の削除が当たらない。soft delete したレコードも `fields` は残っているので、
//!   変換して主キーの列から取る。変換できなかったときだけ `recordName` を割る (それも割れなければ `unresolved_deleted`)。
//! - **`IdolFacet` / `IdolHairstyle` も扱う** (アプリは取り込まない型)。
//! - **色は `#RRGGBB`**。アプリの変換は `#` を剥がした形を返すが、同梱マスタと読み手 (`build_idol_facts` のイメージカラー) は `#` つきを前提にしている。
//! - **`idols.voice_actors` を持つ** (廃止予定の `voiceActors`。声優の履歴の型が CloudKit に来るまで、現在の声優の代わり)。
//! - **孤児の鍵は全部の型で返す** (アプリは単一主キーだけ)。複合主キーの行も CloudKit で物理削除されるため。

use serde_json::{Map, Value};

use crate::domain::ck_record_mapping::{
    deleted_at_millis, is_ingested_record_type, legacy_voice_actors, map_record, mirror_extra_record, record_from_web_services_json,
    CkRecordInput, CkRow, EXTRA_RECORD_TYPES,
};
use crate::domain::mirror_schema::{mirror_table_info, quote};
use crate::domain::sync_planning::parse_composite_record_name;

/// 1 ページ (1 レコードタイプ) の変換結果。
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "web-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../web/wasm/imas-sync-wasm/types/")
)]
pub struct MirrorIngest {
    pub record_type: String,
    /// 写し先の表。
    pub table: String,
    /// 主キーの列。
    pub pk_columns: Vec<String>,
    /// `rows` の各行の列名 (全行で同じ。行が無いときは空)。
    pub columns: Vec<String>,
    /// 書く行。`columns` と同じ並びの値 (文字列 / 数値 / null。真偽は 0 / 1)。
    #[cfg_attr(feature = "web-export", ts(type = "Array<Array<string | number | null>>"))]
    pub rows: Vec<Vec<Value>>,
    /// `rows` を書く SQL。`?1` に `JSON.stringify(rows)` を束縛する。同じ内容の行は書かない
    /// (`meta.changes` が実際に変わった行の数になる)。`rows` が空なら空文字。
    pub upsert_sql: String,
    /// 削除する行の主キーの値 (`pk_columns` と同じ並び)。
    pub deleted_keys: Vec<Vec<String>>,
    /// `deleted_keys` を消す SQL。`?1` に `JSON.stringify(deleted_keys)` を束縛する。
    pub delete_sql: String,
    /// soft delete されていたが、主キーを決められなかった `recordName` (呼ぶ側で警告に出す)。
    pub unresolved_deleted: Vec<String>,
    /// 必須の項目が欠けて捨てた `recordName`。
    pub invalid_record_names: Vec<String>,
    /// この 1 ページで「CloudKit に在る」と分かった行の鍵 (生きている行。単一主キーの表は捨てたレコードも)。単一主キーは値そのもの、複合主キーは値の JSON 配列。
    /// 全件を取り直した完走時の孤児掃除 (`MirrorStep::seen_sql` / `orphan_delete_sql`) が、ここに無い行を消す。
    pub seen_ids: Vec<String>,
}

/// 1 ページ分のレコード (CloudKit Web Services のレコード JSON) を、写し先の行に仕分ける。
/// 写さないレコードタイプでは `None`。`now_millis` は SongVideo の `createdAt` 欠損時の既定値にだけ使う。
pub fn ingest(record_type: &str, record_jsons: &[String], now_millis: i64) -> Option<MirrorIngest> {
    let info = mirror_table_info(record_type)?;
    if !is_ingested_record_type(record_type) && !EXTRA_RECORD_TYPES.contains(&record_type) {
        return None;
    }
    let mut objects: Vec<Map<String, Value>> = Vec::new();
    let mut invalid: Vec<String> = Vec::new();
    let mut deleted_keys: Vec<Vec<String>> = Vec::new();
    let mut unresolved: Vec<String> = Vec::new();
    let mut seen_ids: Vec<String> = Vec::new();

    for json in record_jsons {
        let record = record_from_web_services_json(json);
        let object = row_object(record_type, &record, now_millis);
        if deleted_at_millis(&record).is_some() {
            match deleted_key(&info.pk_columns, &info.table, object.as_ref(), &record.record_name) {
                Some(key) => deleted_keys.push(key),
                None => unresolved.push(record.record_name.clone()),
            }
            continue;
        }
        match object {
            Some(o) => {
                if let Some(key) = seen_key(&info.pk_columns, &o) {
                    seen_ids.push(key);
                }
                objects.push(o);
            }
            None => {
                // 捨てたレコードもサーバーには在る。単一主キーの表は recordName == id なので、孤児と取り違えて対応する行を消さないよう、見えた扱いにする。
                // 複合主キーの表は、捨てた (必須の項目が欠けた) レコードに対応する行がそもそも写せない。
                if info.pk_columns.len() == 1 {
                    seen_ids.push(record.record_name.clone());
                }
                invalid.push(record.record_name);
            }
        }
    }

    let columns: Vec<String> = objects.first().map(|o| o.keys().cloned().collect()).unwrap_or_default();
    let rows: Vec<Vec<Value>> = objects
        .iter()
        .map(|o| columns.iter().map(|c| o.get(c).cloned().unwrap_or(Value::Null)).collect())
        .collect();
    let upsert_sql = if rows.is_empty() { String::new() } else { upsert_sql(&info.table, &info.pk_columns, &columns) };
    let delete_sql = delete_sql(&info.table, &info.pk_columns);
    Some(MirrorIngest {
        record_type: record_type.to_string(),
        table: info.table,
        pk_columns: info.pk_columns,
        columns,
        rows,
        upsert_sql,
        deleted_keys,
        delete_sql,
        unresolved_deleted: unresolved,
        invalid_record_names: invalid,
        seen_ids,
    })
}


/// `records/query` の応答 1 ページ分。レコードの仕分けに、ページングと進み具合の判断に要る値を添えたもの。
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    feature = "web-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../web/wasm/imas-sync-wasm/types/")
)]
pub struct MirrorPage {
    #[serde(flatten)]
    pub ingest: MirrorIngest,
    /// 続きのページのマーカー (`continuationMarker`)。無ければ最後のページ。
    pub marker: Option<String>,
    /// レコード単位のエラー (`serverErrorCode` を持つレコード) の `recordName`。取り込まない。
    pub record_errors: Vec<String>,
    /// ページの**全レコード**の `modifiedAt` (ミリ秒)。`chunk_progress` に渡して、区間の最大とチェックポイントを出す。
    /// 削除したレコードも、捨てたレコードも含める (起点を進めるのは「ここまで見た」ことだから)。
    #[cfg_attr(feature = "web-export", ts(type = "Array<number>"))]
    pub modified_at_millis: Vec<i64>,
}

/// `records/query` の応答本文 (JSON のまま) → [`MirrorPage`]。写さないレコードタイプ・JSON でない本文は `Err`。
///
/// `serverErrorCode` を持つレコードを除くのと `continuationMarker` を取り出すのは通信の側の仕事だが、取り違えると
/// 黙って取りこぼすので、ここで済ませる。
pub fn ingest_page(record_type: &str, body: &str, now_millis: i64) -> Result<MirrorPage, String> {
    let json: Value = serde_json::from_str(body).map_err(|e| format!("応答が JSON でない: {e}"))?;
    let mut jsons: Vec<String> = Vec::new();
    let mut record_errors: Vec<String> = Vec::new();
    let mut modified: Vec<i64> = Vec::new();
    for rec in json.get("records").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
        if rec.get("serverErrorCode").is_some() {
            record_errors.push(rec.get("recordName").and_then(Value::as_str).unwrap_or("?").to_string());
            continue;
        }
        let m = rec.pointer("/fields/modifiedAt");
        if m.and_then(|m| m.get("type")).and_then(Value::as_str) == Some("TIMESTAMP") {
            if let Some(v) = m.and_then(|m| m.get("value")) {
                if let Some(ms) = v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)) {
                    modified.push(ms);
                }
            }
        }
        jsons.push(rec.to_string());
    }
    let ingest = ingest(record_type, &jsons, now_millis).ok_or_else(|| format!("写さないレコードタイプ: {record_type}"))?;
    let marker = json.get("continuationMarker").and_then(Value::as_str).filter(|m| !m.is_empty()).map(str::to_string);
    Ok(MirrorPage { ingest, marker, record_errors, modified_at_millis: modified })
}

/// レコード → 列名 → 値のオブジェクト。必須項目の欠損は `None`。真偽は 0 / 1 にする。
fn row_object(record_type: &str, record: &CkRecordInput, now_millis: i64) -> Option<Map<String, Value>> {
    let value = if EXTRA_RECORD_TYPES.contains(&record_type) {
        mirror_extra_record(record_type, record)?
    } else {
        ck_row_value(&map_record(record_type, record, now_millis)?)?
    };
    let Value::Object(mut o) = value else { return None };
    for v in o.values_mut() {
        if let Value::Bool(b) = v {
            *v = Value::from(i64::from(*b));
        }
    }
    // 色はマスタの形 (`#RRGGBB`) にする。変換規則 (`validated_hex`) は iOS の `HexColor` に合わせて `#` を剥がした形を返すが、
    // 同梱マスタ・`build_idol_facts` などの読み手は `#` つきを前提にしている (剥がしたままだとイメージカラーの項目が出ない)。
    if record_type == "Idol" {
        // 廃止予定の声優の列 (履歴の型が CloudKit に来るまでの代わり)。`CkIdolRow` は読まないので、ここで足す。
        o.insert("voice_actors".to_string(), legacy_voice_actors(record).map_or(Value::Null, Value::String));
    }
    if matches!(record_type, "Brand" | "Idol") {
        if let Some(Value::String(c)) = o.get_mut("color") {
            if !c.is_empty() && !c.starts_with('#') {
                c.insert(0, '#');
            }
        }
    }
    Some(o)
}

fn ck_row_value(row: &CkRow) -> Option<Value> {
    let v = match row {
        CkRow::Brand { row } => serde_json::to_value(row),
        CkRow::Idol { row } => serde_json::to_value(row),
        CkRow::Event { row } => serde_json::to_value(row),
        CkRow::Show { row } => serde_json::to_value(row),
        CkRow::Venue { row } => serde_json::to_value(row),
        CkRow::VenueName { row } => serde_json::to_value(row),
        CkRow::UnitVersion { row } => serde_json::to_value(row),
        CkRow::Costume { row } => serde_json::to_value(row),
        CkRow::CostumeWear { row } => serde_json::to_value(row),
        CkRow::Creator { row } => serde_json::to_value(row),
        CkRow::VenueHall { row } => serde_json::to_value(row),
        CkRow::Song { row } => serde_json::to_value(row),
        CkRow::Unit { row } => serde_json::to_value(row),
        CkRow::IdolBrand { row } => serde_json::to_value(row),
        CkRow::SongArtist { row } => serde_json::to_value(row),
        CkRow::UnitMember { row } => serde_json::to_value(row),
        CkRow::ShowCast { row } => serde_json::to_value(row),
        CkRow::SetlistItem { row } => serde_json::to_value(row),
        CkRow::SetlistPerformer { row } => serde_json::to_value(row),
        CkRow::SongVideo { row } => serde_json::to_value(row),
        CkRow::ShowTicket { row } => serde_json::to_value(row),
        CkRow::TicketSale { row } => serde_json::to_value(row),
    };
    v.ok()
}

/// 孤児掃除に使う「見えた行」の鍵。単一主キーは値そのもの、複合主キーは値の JSON 配列 (`["a","b"]`)。
fn seen_key(pk_columns: &[String], o: &Map<String, Value>) -> Option<String> {
    let values: Option<Vec<&str>> = pk_columns.iter().map(|c| o.get(c).and_then(Value::as_str)).collect();
    let values = values?;
    if values.len() == 1 {
        Some(values[0].to_string())
    } else {
        serde_json::to_string(&values).ok()
    }
}

/// 削除する行の主キーの値。変換できた行の列から取り、できなければ `recordName` から割る。
fn deleted_key(
    pk_columns: &[String],
    table: &str,
    object: Option<&Map<String, Value>>,
    record_name: &str,
) -> Option<Vec<String>> {
    if let Some(o) = object {
        let key: Option<Vec<String>> =
            pk_columns.iter().map(|c| o.get(c).and_then(Value::as_str).map(str::to_string)).collect();
        if key.is_some() {
            return key;
        }
    }
    if pk_columns.len() == 1 {
        // 単一主キーの表は recordName == id (seed の規約)。
        return (!record_name.is_empty()).then(|| vec![record_name.to_string()]);
    }
    parse_composite_record_name(record_name, table, pk_columns.len() as u32)
}

/// `INSERT … SELECT … FROM json_each(?1) … ON CONFLICT … DO UPDATE … WHERE <変わった列がある>`。
///
/// 値の配列 (`[[v, v, …], …]`) を 1 つの束縛で渡すので、D1 の「1 文の束縛は 100 個まで」に当たらない。
/// 主キーの列だけの表 (結合表) は `DO NOTHING`。
pub fn upsert_sql(table: &str, pk_columns: &[String], columns: &[String]) -> String {
    let cols = columns.iter().map(|c| quote(c)).collect::<Vec<_>>().join(", ");
    let selects = (0..columns.len()).map(|i| format!("json_extract(value, '$[{i}]')")).collect::<Vec<_>>().join(", ");
    let pk = pk_columns.iter().map(|c| quote(c)).collect::<Vec<_>>().join(", ");
    let t = quote(table);
    let non_pk: Vec<&String> = columns.iter().filter(|c| !pk_columns.contains(c)).collect();
    let conflict = if non_pk.is_empty() {
        "DO NOTHING".to_string()
    } else {
        let set = non_pk.iter().map(|c| format!("{q} = excluded.{q}", q = quote(c))).collect::<Vec<_>>().join(", ");
        let changed =
            non_pk.iter().map(|c| format!("{t}.{q} IS NOT excluded.{q}", q = quote(c))).collect::<Vec<_>>().join(" OR ");
        format!("DO UPDATE SET {set} WHERE {changed}")
    };
    format!("INSERT INTO {t} ({cols}) SELECT {selects} FROM json_each(?1) WHERE true ON CONFLICT ({pk}) {conflict}")
}

/// `DELETE … WHERE (主キー…) IN (SELECT … FROM json_each(?1))`。`?1` は主キーの値の配列の配列。
pub fn delete_sql(table: &str, pk_columns: &[String]) -> String {
    let pk = pk_columns.iter().map(|c| quote(c)).collect::<Vec<_>>().join(", ");
    let selects =
        (0..pk_columns.len()).map(|i| format!("json_extract(value, '$[{i}]')")).collect::<Vec<_>>().join(", ");
    format!("DELETE FROM {} WHERE ({pk}) IN (SELECT {selects} FROM json_each(?1))", quote(table))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::mirror_schema::{mirror_ddl, mirror_steps};
    use rusqlite::Connection;
    use serde_json::json;

    /// 型ごとの最小のレコード (必須の項目だけ)。`(recordType, recordName, 文字列の項目)`。
    fn samples() -> Vec<(&'static str, &'static str, Vec<(&'static str, &'static str)>)> {
        vec![
            ("Brand", "b1", vec![("name", "ブランド")]),
            ("Creator", "c1", vec![("name", "作家"), ("nameKana", "さっか")]),
            ("Idol", "i1", vec![("name", "アイドル"), ("brandId", "b1")]),
            ("IdolFacet", "f1", vec![("idolId", "i1"), ("facet", "dialect"), ("value", "関西弁")]),
            ("IdolHairstyle", "h1", vec![("idolId", "i1"), ("label", "基本")]),
            ("Event", "e1", vec![("name", "ライブ"), ("eventType", "live")]),
            ("ImasUnit", "u1", vec![("name", "ユニット"), ("brandId", "b1")]),
            ("Venue", "v1", vec![("name", "会場")]),
            ("VenueName", "vn1", vec![("venueId", "v1"), ("name", "旧会場")]),
            ("VenueHall", "vh1", vec![("venueId", "v1"), ("name", "ホール")]),
            ("IdolBrand", "ib1", vec![("idolId", "i1"), ("brandId", "b1")]),
            ("Show", "s1", vec![("eventId", "e1"), ("date", "2026-01-01"), ("name", "初日")]),
            ("UnitVersion", "uv1", vec![("unitId", "u1"), ("name", "版")]),
            ("Song", "g1", vec![("title", "曲"), ("songType", "original")]),
            ("UnitMember", "um1", vec![("unitId", "u1"), ("idolId", "i1")]),
            ("SongArtist", "sa1", vec![("songId", "g1"), ("idolId", "i1")]),
            ("ShowCast", "sc1", vec![("showId", "s1"), ("idolId", "i1")]),
            ("SetlistItem", "sl1", vec![("showId", "s1"), ("songId", "g1")]),
            ("SetlistPerformer", "sp1", vec![("setlistItemId", "sl1"), ("idolId", "i1")]),
            ("Costume", "co1", vec![("name", "衣装")]),
            ("CostumeWear", "cw1", vec![("costumeId", "co1"), ("showId", "s1")]),
            ("ShowTicket", "st1", vec![("showId", "s1"), ("name", "S席")]),
            ("TicketSale", "ts1", vec![("eventId", "e1"), ("kind", "lottery"), ("name", "先行")]),
            ("SongVideo", "sv1", vec![("songId", "g1"), ("youtubeUrl", "https://example.com/v")]),
        ]
    }

    fn record(name: &str, strings: &[(&str, &str)], extra: &[(&str, Value)]) -> String {
        let mut fields = Map::new();
        for (k, v) in strings {
            fields.insert(k.to_string(), json!({"value": v, "type": "STRING"}));
        }
        for (k, v) in extra {
            fields.insert(k.to_string(), v.clone());
        }
        json!({"recordName": name, "fields": fields}).to_string()
    }

    fn changes(conn: &Connection, sql: &str, param: &Value) -> usize {
        conn.execute(sql, [param.to_string()]).unwrap_or_else(|e| panic!("{e}\n{sql}"))
    }

    /// 全部の型について、DDL → 取り込み → 同じ内容の再取り込み (書かない) → 変更 (1 行) → soft delete (消える)。
    #[test]
    fn every_type_round_trips_through_a_real_sqlite() {
        let steps = mirror_steps(&[]);
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&mirror_ddl(&steps.iter().map(|s| s.record_type.clone()).collect::<Vec<_>>())).unwrap();
        let samples = samples();
        assert_eq!(samples.len(), steps.len(), "サンプルの型が全ステップと合っていない");
        for (rt, name, strings) in &samples {
            // 数が必須の型 (ShowTicket の price) だけ数の項目を足す
            let nums: Vec<(&str, Value)> = if *rt == "ShowTicket" { vec![("price", json!({"value": 5000, "type": "INT64"}))] } else { vec![] };
            let r = ingest(rt, &[record(name, strings, &nums)], 1_700_000_000_000).unwrap_or_else(|| panic!("{rt} を写さない"));
            assert_eq!(r.rows.len(), 1, "{rt}: {:?}", r.invalid_record_names);
            assert_eq!(r.columns.len(), r.rows[0].len());
            let rows = json!(r.rows);
            assert_eq!(changes(&conn, &r.upsert_sql, &rows), 1, "{rt}: 初回は 1 行書く");
            assert_eq!(changes(&conn, &r.upsert_sql, &rows), 0, "{rt}: 同じ内容は書かない");
            let count: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM \"{}\"", r.table), [], |x| x.get(0)).unwrap();
            assert_eq!(count, 1, "{rt}");

            // soft delete (deletedAt は TIMESTAMP)。鍵は行から取れて、表から消える。
            let mut with_deleted = nums.clone();
            with_deleted.push(("deletedAt", json!({"value": 1_700_000_000_000i64, "type": "TIMESTAMP"})));
            let tomb = record(name, strings, &with_deleted);
            let d = ingest(rt, &[tomb], 0).unwrap();
            assert!(d.rows.is_empty() && d.upsert_sql.is_empty());
            assert!(d.unresolved_deleted.is_empty(), "{rt}");
            assert_eq!(d.deleted_keys.len(), 1, "{rt}");
            assert_eq!(changes(&conn, &d.delete_sql, &json!(d.deleted_keys)), 1, "{rt}: 消える");
        }
    }

    #[test]
    fn a_changed_field_writes_exactly_that_row() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&mirror_ddl(&["Idol".to_string()])).unwrap();
        let a = ingest("Idol", &[record("i1", &[("name", "A"), ("brandId", "b")], &[])], 0).unwrap();
        let b = ingest("Idol", &[record("i1", &[("name", "B"), ("brandId", "b")], &[])], 0).unwrap();
        assert_eq!(changes(&conn, &a.upsert_sql, &json!(a.rows)), 1);
        assert_eq!(changes(&conn, &b.upsert_sql, &json!(b.rows)), 1);
        let name: String = conn.query_row("SELECT name FROM idols WHERE id='i1'", [], |x| x.get(0)).unwrap();
        assert_eq!(name, "B");
    }

    /// 先頭側の主キーに `-` を含む行 (recordName を前から割ると外れる) も、行から鍵を取るので消える。
    #[test]
    fn tombstone_key_survives_a_hyphen_in_the_first_primary_key() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&mirror_ddl(&["UnitMember".to_string()])).unwrap();
        let live = ingest("UnitMember", &[record("unit_members-3-9days-i1", &[("unitId", "3-9days"), ("idolId", "i1")], &[])], 0).unwrap();
        changes(&conn, &live.upsert_sql, &json!(live.rows));
        let tomb = record(
            "unit_members-3-9days-i1",
            &[("unitId", "3-9days"), ("idolId", "i1")],
            &[("deletedAt", json!({"value": 1, "type": "TIMESTAMP"}))],
        );
        let d = ingest("UnitMember", &[tomb], 0).unwrap();
        assert_eq!(d.deleted_keys, vec![vec!["3-9days".to_string(), "i1".to_string()]]);
        assert_eq!(changes(&conn, &d.delete_sql, &json!(d.deleted_keys)), 1);
    }

    /// 変換できない tombstone は recordName から割る。それも割れなければ unresolved に載る。
    #[test]
    fn tombstone_without_fields_falls_back_to_the_record_name() {
        let tomb = |name: &str| record(name, &[], &[("deletedAt", json!({"value": 1, "type": "TIMESTAMP"}))]);
        let d = ingest("UnitMember", &[tomb("unit_members-u1-i1"), tomb("garbage")], 0).unwrap();
        assert_eq!(d.deleted_keys, vec![vec!["u1".to_string(), "i1".to_string()]]);
        assert_eq!(d.unresolved_deleted, ["garbage"]);
        let single = ingest("Song", &[tomb("g9")], 0).unwrap();
        assert_eq!(single.deleted_keys, vec![vec!["g9".to_string()]]);
    }

    #[test]
    fn invalid_records_are_reported_and_still_count_as_seen() {
        let bad = record("g2", &[("songType", "original")], &[]); // title が無い
        let good = record("g1", &[("title", "曲"), ("songType", "original")], &[]);
        let r = ingest("Song", &[good, bad], 0).unwrap();
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.invalid_record_names, ["g2"]);
        assert_eq!(r.seen_ids, ["g1", "g2"]);
        // 複合主キーの型の鍵は値の JSON 配列 (捨てたレコードは行にならないので入らない)
        let c = ingest("SongArtist", &[record("sa1", &[("songId", "g1"), ("idolId", "i1")], &[]), record("sa2", &[("songId", "g1")], &[])], 0).unwrap();
        assert_eq!(c.seen_ids, [r#"["g1","i1","original"]"#]);
    }

    #[test]
    fn timestamps_keep_their_type_and_unknown_types_are_not_mirrored() {
        // deletedAt が INT64 (型が違う) なら削除にならない = 平坦化の事故を再現させない
        let flat = record("g1", &[("title", "曲"), ("songType", "original")], &[("deletedAt", json!({"value": 5, "type": "INT64"}))]);
        let r = ingest("Song", &[flat], 0).unwrap();
        assert_eq!(r.rows.len(), 1);
        assert!(ingest("CastMember", &[], 0).is_none());
        assert!(ingest("Nope", &[], 0).is_none());
    }

    #[test]
    fn the_legacy_voice_actor_column_is_kept_on_idols() {
        let va = json!({"value": "  中村繪里子 ", "type": "STRING"});
        let r = ingest("Idol", &[record("i1", &[("name", "A"), ("brandId", "b")], &[("voiceActors", va)]), record("i2", &[("name", "B"), ("brandId", "b")], &[])], 0).unwrap();
        let col = r.columns.iter().position(|c| c == "voice_actors").unwrap();
        assert_eq!(r.rows[0][col], json!("中村繪里子"));
        assert_eq!(r.rows[1][col], Value::Null);
    }

    #[test]
    fn colors_keep_the_hash_like_the_bundled_master() {
        let color = |hex: &str| json!({"value": hex, "type": "STRING"});
        let r = ingest("Idol", &[record("i1", &[("name", "A"), ("brandId", "b")], &[("color", color("A453A6"))])], 0).unwrap();
        let col = |n: &str| r.columns.iter().position(|c| c == n).unwrap();
        assert_eq!(r.rows[0][col("color")], json!("#A453A6"));
        let r = ingest("Brand", &[record("b", &[("name", "B")], &[("color", color("#0fbe94"))])], 0).unwrap();
        assert_eq!(r.rows[0][r.columns.iter().position(|c| c == "color").unwrap()], json!("#0fbe94"));
        // 色でない値は色なし (変換規則のまま)
        let r = ingest("Brand", &[record("b", &[("name", "B")], &[("color", color("not-a-color"))])], 0).unwrap();
        assert_eq!(r.rows[0][r.columns.iter().position(|c| c == "color").unwrap()], Value::Null);
    }

    /// 全件を取り直したあと、見えなかった行 (単一主キーも複合主キーも) だけが孤児として消える。
    #[test]
    fn orphan_sweep_removes_only_rows_that_were_not_seen() {
        let steps = mirror_steps(&["Song".to_string(), "SongArtist".to_string()]);
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(&mirror_ddl(&["Song".to_string(), "SongArtist".to_string()])).unwrap();
        conn.execute_batch(&format!("{}; ", crate::domain::mirror_schema::MIRROR_SEEN_DDL)).unwrap();
        conn.execute_batch(
            "INSERT INTO songs (id, title, song_type) VALUES ('g1','a','x'), ('g2','b','x'), ('g3','c','x');
             INSERT INTO song_artists (song_id, idol_id, role) VALUES ('g1','i1','original'), ('g1','i2','original'), ('g2','i1','original');",
        )
        .unwrap();
        let song = &steps[0];
        let artist = &steps[1];
        // 実行 7 で見えたのは g1, g3 と (g1,i1,original)
        conn.execute(&song.seen_sql, rusqlite::params!["Song", 7, json!(["g1", "g3"]).to_string()]).unwrap();
        conn.execute(&artist.seen_sql, rusqlite::params!["SongArtist", 7, json!([r#"["g1","i1","original"]"#]).to_string()]).unwrap();
        let probe = |s: &crate::domain::mirror_schema::MirrorStep| -> (i64, i64, i64) {
            conn.query_row(&s.orphan_probe_sql, rusqlite::params![s.record_type, 7], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap()
        };
        assert_eq!(probe(song), (2, 3, 1)); // seen, total, orphans (g2)
        assert_eq!(probe(artist), (1, 3, 2));
        assert_eq!(conn.execute(&song.orphan_delete_sql, rusqlite::params!["Song", 7]).unwrap(), 1);
        assert_eq!(conn.execute(&artist.orphan_delete_sql, rusqlite::params!["SongArtist", 7]).unwrap(), 2);
        let left: Vec<String> = conn.prepare("SELECT id FROM songs ORDER BY id").unwrap().query_map([], |r| r.get(0)).unwrap().filter_map(Result::ok).collect();
        assert_eq!(left, ["g1", "g3"]);
        let artists: i64 = conn.query_row("SELECT COUNT(*) FROM song_artists", [], |r| r.get(0)).unwrap();
        assert_eq!(artists, 1);
    }

    #[test]
    fn booleans_become_integers_and_reals_stay_reals() {
        let r = ingest(
            "Idol",
            &[record(
                "i1",
                &[("name", "A"), ("brandId", "b")],
                &[
                    ("isExternal", json!({"value": 1, "type": "INT64"})),
                    ("height", json!({"value": 154.5, "type": "DOUBLE"})),
                ],
            )],
            0,
        )
        .unwrap();
        let col = |n: &str| r.columns.iter().position(|c| c == n).unwrap();
        assert_eq!(r.rows[0][col("is_external")], json!(1));
        assert_eq!(r.rows[0][col("height")], json!(154.5));
    }

    #[test]
    fn a_response_page_gives_rows_marker_errors_and_modified_times() {
        let ts = |ms: i64| json!({"value": ms, "type": "TIMESTAMP"});
        let a: Value = serde_json::from_str(&record("b1", &[("name", "A")], &[("modifiedAt", ts(100))])).unwrap();
        let del: Value = serde_json::from_str(&record("b2", &[("name", "B")], &[("modifiedAt", ts(300)), ("deletedAt", ts(300))])).unwrap();
        let bad = json!({"recordName": "b3", "serverErrorCode": "ACCESS_DENIED", "reason": "x"});
        let body = json!({"records": [a, del, bad], "continuationMarker": "abc"}).to_string();
        let p = ingest_page("Brand", &body, 0).unwrap();
        assert_eq!(p.ingest.rows.len(), 1);
        assert_eq!(p.ingest.deleted_keys, vec![vec!["b2".to_string()]]);
        assert_eq!(p.marker.as_deref(), Some("abc"));
        assert_eq!(p.record_errors, ["b3"]);
        assert_eq!(p.modified_at_millis, [100, 300]);
        let last = ingest_page("Brand", &json!({"records": []}).to_string(), 0).unwrap();
        assert_eq!(last.marker, None);
        assert!(ingest_page("Brand", "not json", 0).is_err());
        assert!(ingest_page("CastMember", "{}", 0).is_err());
    }
}
