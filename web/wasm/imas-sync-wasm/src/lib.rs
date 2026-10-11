//! CloudKit の公開データを「マスタと同じ構造の SQLite」に写すための薄いラッパ。
//!
//! **規則はここに無い。** 全部 `imas-core` の domain の関数をそのまま呼び、JSON で受け渡すだけ:
//!
//! | 用途 | 呼ぶもの | 中身 (imas-core) |
//! |---|---|---|
//! | 写す表の DDL | [`mirror_ddl`] / [`mirror_index_ddl`] / [`mirror_steps`] | `domain::mirror_schema` |
//! | レコード → 行 | [`ingest_page`] | `domain::mirror_ingest` (`ck_record_mapping` と同じ変換) |
//! | 差分同期の判定 | [`startup_plan`] [`run_start_plan`] [`step_start`] [`next_chunk_action`] [`chunk_progress`] [`completion_plan`] … | `domain::sync_planning` / `sync_decisions` / `mirror_policy` |
//! | アイドルの項目の組み立て | [`IdolFacts`] | `domain::mirror_load` → `domain::idol_facets::build_idol_facts` |
//!
//! 通信 (HTTP・トークン) と SQL の実行は呼ぶ側。手順は `docs/ARCHITECTURE-mirror.md`。
//!
//! # 時刻の単位
//!
//! 同期の判定の時刻は imas-core と同じ**秒 (f64)**。呼ぶ側は `ms / 1000` で渡し、`Math.round(sec * 1000)` で戻す
//! (ミリ秒のまま渡すと、境界の巻き戻し幅 1ms が 1000 倍ずれて境界のレコードを取りこぼす)。
//! `ingest_page` が返す `modifiedAtMillis` だけがミリ秒 (CloudKit の TIMESTAMP のまま)。

use imas_core::agent::tools::publish::idol_facts_document;
use imas_core::domain::idol_facets::{definitions_json, TagFacetInput};
use imas_core::domain::mirror_ingest;
use imas_core::domain::mirror_load::{MirrorLoader, LOADABLE_TABLES};
use imas_core::domain::mirror_policy;
use imas_core::domain::mirror_schema;
use imas_core::domain::snapshot::Snapshot;
use imas_core::domain::sync_decisions::{chunk_progress as core_chunk_progress, SyncErrorSignal};
use imas_core::domain::sync_planning as planning;
use serde::de::DeserializeOwned;
use serde::Serialize;
use wasm_bindgen::prelude::*;

fn err(prefix: &str, e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&format!("{prefix}: {e}"))
}

fn parse<T: DeserializeOwned>(what: &str, json: &str) -> Result<T, JsValue> {
    serde_json::from_str(json).map_err(|e| err(&format!("{what}を読めない"), e))
}

fn to_json<T: Serialize>(value: &T) -> Result<String, JsValue> {
    serde_json::to_string(value).map_err(|e| err("結果を JSON にできない", e))
}

// ---------------------------------------------------------------------------
// 表の定義
// ---------------------------------------------------------------------------

/// 写すステップ (FK 依存順)。`available_json` は `["Idol","Song"]` のようなレコードタイプの配列。`[]` なら全部。
/// 結果は `MirrorStep[]` の JSON。
#[wasm_bindgen]
pub fn mirror_steps(available_json: &str) -> Result<String, JsValue> {
    let available: Vec<String> = parse("レコードタイプの配列", available_json)?;
    to_json(&mirror_schema::mirror_steps(&available))
}

/// 写す表の CREATE TABLE (`;` 区切り。FK と複合 UNIQUE は外してある)。引数は `mirror_steps` のレコードタイプの配列。
#[wasm_bindgen]
pub fn mirror_ddl(record_types_json: &str) -> Result<String, JsValue> {
    let types: Vec<String> = parse("レコードタイプの配列", record_types_json)?;
    Ok(mirror_schema::mirror_ddl(&types))
}

/// 孤児掃除のための「見えた鍵」の表 (`mirror_seen`) の CREATE TABLE。状態の表なので `mirror_ddl` には含まれない。
#[wasm_bindgen]
pub fn mirror_seen_ddl() -> String {
    mirror_schema::MIRROR_SEEN_DDL.to_string()
}

/// 同じ表につく一意でない索引の CREATE INDEX。
#[wasm_bindgen]
pub fn mirror_index_ddl(record_types_json: &str) -> Result<String, JsValue> {
    let types: Vec<String> = parse("レコードタイプの配列", record_types_json)?;
    Ok(mirror_schema::mirror_index_ddl(&types))
}

// ---------------------------------------------------------------------------
// レコード → 行
// ---------------------------------------------------------------------------

/// `records/query` の応答本文 (JSON のまま) 1 ページ → `MirrorPage` の JSON。
/// 行・書く SQL・削除する鍵・孤児判定の id・続きのマーカー・`modifiedAt` の一覧が返る。
#[wasm_bindgen]
pub fn ingest_page(record_type: &str, response_body: &str, now_millis: f64) -> Result<String, JsValue> {
    let page = mirror_ingest::ingest_page(record_type, response_body, now_millis as i64).map_err(|e| err("ページを写せない", e))?;
    to_json(&page)
}

// ---------------------------------------------------------------------------
// 差分同期の判定 (時刻は秒)
// ---------------------------------------------------------------------------

/// 起動時の同期モード (`SyncStartupState` → `SyncStartupPlan`)。
#[wasm_bindgen]
pub fn startup_plan(state_json: &str) -> Result<String, JsValue> {
    let state: planning::SyncStartupState = parse("起動時の状態", state_json)?;
    to_json(&planning::startup_plan(&state))
}

/// 差分の実行の起点 (前回の起点から 2 時間遡る)。`None` (フル) はそのまま。
#[wasm_bindgen]
pub fn incremental_since(modified_since_epoch: Option<f64>) -> Option<f64> {
    mirror_policy::incremental_since(modified_since_epoch)
}

/// 実行の起点 (`SyncRunStart`)。`persisted_done_steps_json` は `string[]`。
#[wasm_bindgen]
pub fn run_start_plan(
    is_full_sync: bool,
    sync_start_epoch: f64,
    pending_full_start_epoch: Option<f64>,
    persisted_done_steps_json: &str,
) -> Result<String, JsValue> {
    let done: Vec<String> = parse("完了したステップ", persisted_done_steps_json)?;
    to_json(&planning::run_start_plan(is_full_sync, sync_start_epoch, pending_full_start_epoch, &done))
}

/// ステップの取得起点 (`SyncStepStart`)。`backfilled` が偽の型は差分の実行でも epoch から読む。
#[wasm_bindgen]
pub fn step_start(
    record_type: &str,
    is_full_sync: bool,
    done_steps_json: &str,
    checkpoint_epoch: Option<f64>,
    modified_since_epoch: Option<f64>,
    backfilled: bool,
) -> Result<String, JsValue> {
    let done: Vec<String> = parse("完了したステップ", done_steps_json)?;
    to_json(&mirror_policy::step_start(record_type, is_full_sync, &done, checkpoint_epoch, modified_since_epoch, backfilled))
}

/// チャンクを取り込んだあとの次の一手 (`SyncChunkAction`)。
/// `added_since_restart` は**実際に変わった行の数** (取得件数ではない。取得件数を渡すと張り直しが終わらない)。
#[wasm_bindgen]
pub fn next_chunk_action(has_next_cursor: bool, added_since_restart: u32, max_epoch_since_restart: Option<f64>) -> Result<String, JsValue> {
    to_json(&planning::next_chunk_action(has_next_cursor, added_since_restart, max_epoch_since_restart))
}

/// チャンクを取り込んだあとの区間の最大とチェックポイント (`SyncChunkProgress`)。`chunk_modified_epochs` は秒。
#[wasm_bindgen]
pub fn chunk_progress(previous_max_since_restart: Option<f64>, chunk_modified_epochs: Vec<f64>) -> Result<String, JsValue> {
    to_json(&core_chunk_progress(previous_max_since_restart, &chunk_modified_epochs))
}

/// 実行を終えたときの後始末 (`SyncCompletionState` → `SyncCompletion`)。
#[wasm_bindgen]
pub fn completion_plan(state_json: &str) -> Result<String, JsValue> {
    let state: planning::SyncCompletionState = parse("完了時の状態", state_json)?;
    to_json(&planning::completion_plan(&state))
}

/// 取得の失敗の分類 (`MirrorErrorKind` の文字列)。`signal_json` は `SyncErrorSignal`、`has_marker` は続きのページの失敗か。
#[wasm_bindgen]
pub fn classify_fetch_error(signal_json: &str, has_marker: bool) -> Result<String, JsValue> {
    let signal: SyncErrorSignal = parse("失敗の情報", signal_json)?;
    let kind = mirror_policy::classify_fetch_error(&signal, has_marker);
    // enum 単体の JSON は `"unknown_record_type"`。引用符を外して文字列で返す。
    Ok(to_json(&kind)?.trim_matches('"').to_string())
}

/// このステップの完走で孤児掃除をしてよいか。
#[wasm_bindgen]
pub fn should_delete_orphans(record_type: &str, started_from_epoch: bool) -> bool {
    mirror_policy::should_delete_orphans(record_type, started_from_epoch)
}

/// 孤児を消してよい件数か (型の行数の 5%、小さい型は 3 件まで)。
#[wasm_bindgen]
pub fn orphan_delete_allowed(orphans: u32, total: u32) -> bool {
    mirror_policy::orphan_delete_allowed(orphans as usize, total as usize)
}

// ---------------------------------------------------------------------------
// アイドルの項目の組み立て
// ---------------------------------------------------------------------------

/// 写した表の行を溜めて Snapshot を組み、アイドルの項目の束を返すハンドル。
///
/// 1. `new IdolFacts()`
/// 2. `loadable_tables()` の表を `SELECT *` で読み、ページごとに `add_rows(table, JSON.stringify(rows))`
/// 3. `finish()`
/// 4. `facts(idolId, tagsJson)` を人数ぶん
///
/// 溜めるのは型つきの行 (JSON のままではない) なので、ページに分ければ JS 側のメモリは 1 ページ分で済む。
#[wasm_bindgen]
pub struct IdolFacts {
    loader: Option<MirrorLoader>,
    snap: Option<Snapshot>,
}

#[wasm_bindgen]
impl IdolFacts {
    #[wasm_bindgen(constructor)]
    pub fn new() -> IdolFacts {
        IdolFacts { loader: Some(MirrorLoader::new()), snap: None }
    }

    /// 表の行 (列名 → 値のオブジェクトの JSON 配列) を足す。足した行数を返す。
    pub fn add_rows(&mut self, table: &str, rows_json: &str) -> Result<usize, JsValue> {
        let loader = self.loader.as_mut().ok_or_else(|| JsValue::from_str("finish のあとに行は足せない"))?;
        let rows: Vec<serde_json::Value> = parse(&format!("{table} の行"), rows_json)?;
        loader.add_rows(table, rows).map_err(|e| JsValue::from_str(&e))
    }

    /// 溜めた行から Snapshot を組む (索引の構築。CPU を使うのはここ)。
    pub fn finish(&mut self) -> Result<(), JsValue> {
        let loader = self.loader.take().ok_or_else(|| JsValue::from_str("finish は 1 回だけ"))?;
        self.snap = Some(loader.finish());
        Ok(())
    }

    /// 写しにいるアイドルの id。
    pub fn idol_ids(&self) -> Result<Vec<String>, JsValue> {
        Ok(self.snapshot()?.idols.iter().map(|i| i.id.clone()).collect())
    }

    /// アイドル 1 人の項目の束 (`GET /v1/idols/:id/facts` と同じ文書の JSON)。いなければ `undefined`。
    /// `tags_json` はタグの票 (`TagFacetInput[]`)。空文字なら無し。話し方は imas-core が持つ公式データを使う。
    pub fn facts(&self, idol_id: &str, tags_json: &str) -> Result<Option<String>, JsValue> {
        let snap = self.snapshot()?;
        let tags: Vec<TagFacetInput> =
            if tags_json.trim().is_empty() { Vec::new() } else { parse("タグの票", tags_json)? };
        match idol_facts_document(snap, idol_id, &tags) {
            Ok(doc) => Ok(Some(to_json(&doc)?)),
            Err(_) => Ok(None),
        }
    }

    fn snapshot(&self) -> Result<&Snapshot, JsValue> {
        self.snap.as_ref().ok_or_else(|| JsValue::from_str("finish の前には読めない"))
    }
}

impl Default for IdolFacts {
    fn default() -> Self {
        Self::new()
    }
}

/// `IdolFacts.add_rows` が受ける表。
#[wasm_bindgen]
pub fn loadable_tables() -> Vec<String> {
    LOADABLE_TABLES.iter().map(|t| t.to_string()).collect()
}

/// 項目の定義 (`imas-core/facets.json` と同じ。キー・カテゴリ・ラベル・単位・全件側の対応)。
#[wasm_bindgen]
pub fn facet_definitions() -> Result<String, JsValue> {
    to_json(&definitions_json())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planning_json_round_trips() {
        let plan = startup_plan(
            r#"{"hasPendingFullSync":false,"localDataEmpty":false,"lastFullSyncEpoch":100.0,"lastSyncEpoch":5000.5,"nowEpoch":6000.0,"fullSyncIntervalSeconds":604800.0}"#,
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&plan).unwrap();
        assert_eq!(v["mode"], "incremental");
        assert_eq!(v["modifiedSinceEpoch"], 5000.5);
        assert_eq!(v["reason"], "incremental");
    }

    #[test]
    fn chunk_action_is_tagged() {
        let v: serde_json::Value = serde_json::from_str(&next_chunk_action(false, 3, Some(10.0)).unwrap()).unwrap();
        assert_eq!(v["kind"], "restart_from");
        assert_eq!(v["startEpoch"], 10.0 - 0.001);
        let v: serde_json::Value = serde_json::from_str(&next_chunk_action(true, 0, None).unwrap()).unwrap();
        assert_eq!(v["kind"], "continue_cursor");
    }

    #[test]
    fn steps_ddl_and_page_come_through() {
        let steps: Vec<serde_json::Value> = serde_json::from_str(&mirror_steps("[]").unwrap()).unwrap();
        assert_eq!(steps[0]["recordType"], "Brand");
        let types: Vec<String> = steps.iter().map(|s| s["recordType"].as_str().unwrap().to_string()).collect();
        let ddl = mirror_ddl(&serde_json::to_string(&types).unwrap()).unwrap();
        assert!(ddl.contains("CREATE TABLE idol_facets"));
        let body = r#"{"records":[{"recordName":"b1","fields":{"name":{"value":"B","type":"STRING"},"modifiedAt":{"value":42,"type":"TIMESTAMP"}}}]}"#;
        let page: serde_json::Value = serde_json::from_str(&ingest_page("Brand", body, 0.0).unwrap()).unwrap();
        assert_eq!(page["table"], "brands");
        assert_eq!(page["modifiedAtMillis"][0], 42);
        assert!(page["upsertSql"].as_str().unwrap().starts_with("INSERT INTO \"brands\""));
    }

    #[test]
    fn facts_come_out_of_loaded_rows() {
        let mut f = IdolFacts::new();
        f.add_rows("brands", r#"[{"id":"ml","name":"ミリオンライブ！","short_name":"ML","color":null,"sort_order":1}]"#).unwrap();
        f.add_rows("idols", r#"[{"id":"ml_x","brand_id":"ml","name":"春日未来","sort_order":1,"is_external":0,"age":14}]"#).unwrap();
        f.finish().unwrap();
        assert_eq!(f.idol_ids().unwrap(), ["ml_x"]);
        let doc: serde_json::Value = serde_json::from_str(&f.facts("ml_x", "").unwrap().unwrap()).unwrap();
        assert_eq!(doc["categories"]["profile"]["age"]["value"], 14);
        assert!(f.facts("nobody", "").unwrap().is_none());
    }
}
