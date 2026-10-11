//! 写した表の行 → [`Snapshot`]。マスタの Snapshot と**同じ構造**になるので、`build_idol_facts` などの
//! domain の関数がそのまま使える。
//!
//! 通常のローダ (`outbound::sqlite_loader`) は SQLite のファイルを開く。写し先がサーバーの SQLite (Cloudflare D1 など) で
//! ファイルを開けないとき、呼ぶ側は表を読み出した行 (列名 → 値のオブジェクト) をここに渡す。
//!
//! ```text
//! let mut loader = MirrorLoader::new();
//! loop { loader.add_rows("song_artists", rows_of_this_page)?; }   // 表ごとに、何度に分けて渡してもよい
//! let snap = loader.finish();                                      // 並べ直し・添字リンク・孤児の読み飛ばし・索引
//! ```
//!
//! 行は渡された時点で型つきの行に直して持つ (JSON のまま溜めない)ので、ページに分けて渡せばメモリは Snapshot 1 つ分で済む。
//! SQLite の `SELECT *` の行をそのまま渡せる (真偽は 0 / 1 の整数でよい)。並び順・孤児の扱い・既定値は
//! `outbound::sqlite_loader` と同じ (テストが、同じ DB から両方の経路で組んだ Snapshot と項目の束が一致することを確かめる)。

use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::domain::snapshot::{
    Brand, Event, Idol, IdolFacetRow, IdolHairstyleRow, IdolVoiceActor, SetlistItem, Show, Snapshot, Song, Unit,
};
use crate::domain::snapshot_build::{build, RawTables};

/// [`MirrorLoader::add_rows`] が受ける表。`Snapshot` の項目の束 (`build_idol_facts`) と一覧・検索が読む分。
pub const LOADABLE_TABLES: &[&str] = &[
    "brands",
    "idols",
    "idol_brands",
    "idol_facets",
    "idol_hairstyles",
    "idol_voice_actors",
    "units",
    "unit_members",
    "songs",
    "song_artists",
    "events",
    "shows",
    "show_cast",
    "setlist_items",
    "setlist_performers",
];

/// 行を溜めて Snapshot を組む。
#[derive(Default)]
pub struct MirrorLoader {
    brands: Vec<Brand>,
    idols: Vec<Idol>,
    idol_brands: Vec<(String, String, Option<i64>)>,
    idol_facets: Vec<IdolFacetRow>,
    idol_hairstyles: Vec<IdolHairstyleRow>,
    voice_actors: Vec<VoiceActorRow>,
    /// `idols.voice_actors` (廃止予定の声優の列)。履歴の行が無いアイドルの現在の声優にする。
    legacy_voice_actors: Vec<(String, String)>,
    units: Vec<Unit>,
    unit_members: Vec<(String, String)>,
    songs: Vec<Song>,
    song_artists: Vec<(String, String, Option<String>)>,
    events: Vec<Event>,
    shows: Vec<ShowRow>,
    show_cast: Vec<(String, String, Option<String>)>,
    setlist_items: Vec<SetlistItemRow>,
    setlist_performers: Vec<(String, String)>,
}

// 添字リンクを持つ表は、親が全部そろうまで解決できないので文字列の id のまま溜める。
#[derive(serde::Deserialize)]
struct ShowRow {
    id: String,
    event_id: String,
    name: String,
    date: String,
    venue: Option<String>,
    venue_city: Option<String>,
    start_time: Option<String>,
    sort_order: Option<i64>,
    performer_type: Option<String>,
    venue_id: Option<String>,
    hall: Option<String>,
    stream_platform: Option<String>,
    venue_mode: Option<String>,
}

#[derive(serde::Deserialize)]
struct SetlistItemRow {
    id: String,
    show_id: String,
    song_id: String,
    position: Option<i64>,
    section: Option<String>,
    notes: Option<String>,
    unit_name: Option<String>,
}

#[derive(serde::Deserialize)]
struct VoiceActorRow {
    id: String,
    idol_id: String,
    name: String,
    valid_from: Option<String>,
    valid_to: Option<String>,
}

/// 髪型の行。`styles` / `accessories` は表では '、' 区切りの 1 文字列。
#[derive(serde::Deserialize)]
struct HairstyleRow {
    id: String,
    idol_id: String,
    label: String,
    is_main: Option<i64>,
    hair_color: Option<String>,
    hair_color_secondary: Option<String>,
    hair_length: Option<String>,
    styles: Option<String>,
    bangs: Option<String>,
    accessories: Option<String>,
    origin: Option<String>,
    source_note: Option<String>,
    sort_order: Option<i64>,
}

#[derive(serde::Deserialize)]
struct FacetRow {
    id: String,
    idol_id: String,
    facet: String,
    value: String,
    origin: Option<String>,
    source_note: Option<String>,
    sort_order: Option<i64>,
}

impl MirrorLoader {
    pub fn new() -> Self {
        Self::default()
    }

    /// その表を受けるか。
    pub fn supports(table: &str) -> bool {
        LOADABLE_TABLES.contains(&table)
    }

    /// 表の行 (列名 → 値のオブジェクト) を足す。足した行数を返す。受けない表・読めない行は `Err` (どの表の何行目かを添える)。
    pub fn add_rows(&mut self, table: &str, rows: Vec<Value>) -> Result<usize, String> {
        let n = rows.len();
        match table {
            "brands" => extend(&mut self.brands, table, rows, &[])?,
            "idols" => {
                for r in &rows {
                    if let (Some(id), Some(name)) = (r.get("id").and_then(Value::as_str), r.get("voice_actors").and_then(Value::as_str)) {
                        if !name.trim().is_empty() {
                            self.legacy_voice_actors.push((id.to_string(), name.trim().to_string()));
                        }
                    }
                }
                extend(&mut self.idols, table, rows, &["is_external"])?
            }
            "units" => extend(&mut self.units, table, rows, &["is_permanent"])?,
            "songs" => extend(&mut self.songs, table, rows, &["is_collab", "has_kamisabi_card"])?,
            "events" => extend(&mut self.events, table, rows, &["is_streaming", "is_solo"])?,
            "shows" => extend(&mut self.shows, table, rows, &[])?,
            "setlist_items" => extend(&mut self.setlist_items, table, rows, &[])?,
            "idol_voice_actors" => extend(&mut self.voice_actors, table, rows, &[])?,
            "idol_facets" => {
                let mut parsed: Vec<FacetRow> = Vec::new();
                extend(&mut parsed, table, rows, &[])?;
                self.idol_facets.extend(parsed.into_iter().map(|r| IdolFacetRow {
                    id: r.id,
                    idol_id: r.idol_id,
                    facet: r.facet,
                    value: r.value,
                    origin: r.origin.unwrap_or_else(|| "official".into()),
                    source_note: r.source_note,
                    sort_order: r.sort_order.unwrap_or(0),
                }));
            }
            "idol_hairstyles" => {
                let mut parsed: Vec<HairstyleRow> = Vec::new();
                extend(&mut parsed, table, rows, &[])?;
                self.idol_hairstyles.extend(parsed.into_iter().map(|r| IdolHairstyleRow {
                    id: r.id,
                    idol_id: r.idol_id,
                    label: r.label,
                    is_main: r.is_main.unwrap_or(0) != 0,
                    hair_color: r.hair_color,
                    hair_color_secondary: r.hair_color_secondary,
                    hair_length: r.hair_length,
                    styles: split_multi(r.styles),
                    bangs: r.bangs,
                    accessories: split_multi(r.accessories),
                    origin: r.origin.unwrap_or_else(|| "official".into()),
                    source_note: r.source_note,
                    sort_order: r.sort_order.unwrap_or(0),
                }));
            }
            "song_artists" => {
                for (i, r) in rows.iter().enumerate() {
                    self.song_artists.push((text(r, "song_id", table, i)?, text(r, "idol_id", table, i)?, opt_text(r, "role")));
                }
            }
            "setlist_performers" => {
                for (i, r) in rows.iter().enumerate() {
                    self.setlist_performers.push((text(r, "setlist_item_id", table, i)?, text(r, "idol_id", table, i)?));
                }
            }
            "show_cast" => {
                for (i, r) in rows.iter().enumerate() {
                    self.show_cast.push((text(r, "show_id", table, i)?, text(r, "idol_id", table, i)?, opt_text(r, "cast_role")));
                }
            }
            "unit_members" => {
                for (i, r) in rows.iter().enumerate() {
                    self.unit_members.push((text(r, "unit_id", table, i)?, text(r, "idol_id", table, i)?));
                }
            }
            "idol_brands" => {
                for (i, r) in rows.iter().enumerate() {
                    self.idol_brands.push((
                        text(r, "idol_id", table, i)?,
                        text(r, "brand_id", table, i)?,
                        r.get("is_primary").and_then(Value::as_i64),
                    ));
                }
            }
            other => return Err(format!("読み込めない表: {other}")),
        }
        Ok(n)
    }

    /// 溜めた行から Snapshot を組む。並びは `outbound::sqlite_loader` と同じ (主キー順)、親の無い行は読み飛ばす。
    pub fn finish(mut self) -> Snapshot {
        self.songs.sort_by(|a, b| a.id.cmp(&b.id));
        self.idols.sort_by(|a, b| a.id.cmp(&b.id));
        self.events.sort_by(|a, b| a.id.cmp(&b.id));
        self.units.sort_by(|a, b| a.id.cmp(&b.id));
        self.brands.sort_by(|a, b| a.id.cmp(&b.id));
        self.shows.sort_by(|a, b| a.id.cmp(&b.id));
        self.setlist_items.sort_by(|a, b| a.id.cmp(&b.id));
        self.voice_actors.sort_by(|a, b| a.id.cmp(&b.id));
        self.song_artists.sort();
        self.setlist_performers.sort();
        self.show_cast.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        self.unit_members.sort();
        self.idol_brands.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        self.idol_facets.sort_by(|a, b| {
            (&a.idol_id, &a.facet, a.sort_order, &a.id).cmp(&(&b.idol_id, &b.facet, b.sort_order, &b.id))
        });
        self.idol_hairstyles
            .sort_by(|a, b| (&a.idol_id, a.sort_order, &a.id).cmp(&(&b.idol_id, b.sort_order, &b.id)));

        let index = |ids: Vec<&String>| -> HashMap<String, u32> {
            ids.into_iter().enumerate().map(|(i, id)| (id.clone(), i as u32)).collect()
        };
        let song_index = index(self.songs.iter().map(|s| &s.id).collect());
        let idol_index = index(self.idols.iter().map(|s| &s.id).collect());
        let event_index = index(self.events.iter().map(|s| &s.id).collect());

        let shows: Vec<Show> = self
            .shows
            .into_iter()
            .filter_map(|r| {
                let event = *event_index.get(&r.event_id)?;
                Some(Show {
                    id: r.id,
                    event,
                    name: r.name,
                    date: r.date,
                    venue: r.venue,
                    venue_city: r.venue_city,
                    start_time: r.start_time,
                    sort_order: r.sort_order.unwrap_or(0),
                    performer_type: r.performer_type,
                    venue_id: r.venue_id,
                    hall: r.hall,
                    stream_platform: r.stream_platform,
                    venue_mode: r.venue_mode,
                    has_streaming: None,
                    has_live_viewing: None,
                })
            })
            .collect();
        let show_index = index(shows.iter().map(|s| &s.id).collect());
        let setlist_items: Vec<SetlistItem> = self
            .setlist_items
            .into_iter()
            .filter_map(|r| {
                Some(SetlistItem {
                    show: *show_index.get(&r.show_id)?,
                    song: *song_index.get(&r.song_id)?,
                    id: r.id,
                    position: r.position.unwrap_or(0),
                    section: r.section,
                    notes: r.notes,
                    unit_name: r.unit_name,
                })
            })
            .collect();
        let mut idol_voice_actors: Vec<IdolVoiceActor> = self
            .voice_actors
            .into_iter()
            .filter_map(|r| {
                Some(IdolVoiceActor {
                    idol: *idol_index.get(&r.idol_id)?,
                    id: r.id,
                    name: r.name,
                    valid_from: r.valid_from,
                    valid_to: r.valid_to,
                })
            })
            .collect();
        // 履歴の行が無いアイドルは、廃止予定の列の声優を「いま」の声優にする (期間なし = 現任)。
        let with_history: std::collections::HashSet<u32> = idol_voice_actors.iter().map(|v| v.idol).collect();
        for (idol_id, name) in self.legacy_voice_actors {
            if let Some(&idol) = idol_index.get(&idol_id) {
                if !with_history.contains(&idol) {
                    idol_voice_actors.push(IdolVoiceActor { id: format!("legacy:{idol_id}"), idol, name, valid_from: None, valid_to: None });
                }
            }
        }
        idol_voice_actors.sort_by(|a, b| a.id.cmp(&b.id));

        build(RawTables {
            songs: self.songs,
            idols: self.idols,
            events: self.events,
            units: self.units,
            brands: self.brands,
            creators: Vec::new(),
            venues: Vec::new(),
            staff: Vec::new(),
            anniversaries: Vec::new(),
            meta: Default::default(),
            shows,
            setlist_items,
            venue_names: Vec::new(),
            venue_halls: Vec::new(),
            idol_voice_actors,
            event_releases: Vec::new(),
            costumes: Vec::new(),
            costume_wears: Vec::new(),
            ticket_sales: Vec::new(),
            show_archives: Vec::new(),
            idol_facets: self.idol_facets,
            idol_hairstyles: self.idol_hairstyles,
            song_artists: self.song_artists,
            setlist_performers: self.setlist_performers,
            show_cast: self.show_cast,
            unit_members: self.unit_members,
            idol_brands: self.idol_brands,
        })
    }
}

/// 行を型つきの行に直して足す。`bools` の列は 0 / 1 の整数を真偽にする (SQLite の表現)。
fn extend<T: DeserializeOwned>(out: &mut Vec<T>, table: &str, rows: Vec<Value>, bools: &[&str]) -> Result<(), String> {
    out.reserve(rows.len());
    for (i, mut row) in rows.into_iter().enumerate() {
        if let Value::Object(o) = &mut row {
            for key in bools {
                if let Some(v) = o.get_mut(*key) {
                    if let Some(n) = v.as_i64() {
                        *v = Value::Bool(n != 0);
                    } else if v.is_null() {
                        o.remove(*key); // 列の既定値に任せる (型の側が必須ならここで落ちる)
                    }
                }
            }
        }
        out.push(serde_json::from_value(row).map_err(|e| format!("{table} の {i} 行目を読めない: {e}"))?);
    }
    Ok(())
}

fn text(row: &Value, key: &str, table: &str, i: usize) -> Result<String, String> {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("{table} の {i} 行目に {key} が無い"))
}

fn opt_text(row: &Value, key: &str) -> Option<String> {
    row.get(key).and_then(Value::as_str).map(str::to_string)
}

/// '、' 区切りの複数値の列 → 配列 (`sqlite_loader::split_multi` と同じ)。
fn split_multi(v: Option<String>) -> Vec<String> {
    v.map(|t| t.split('、').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::idol_facets::{build_idol_facts, PersonaInput};
    use crate::test_support::{bundle_conn, bundle_snapshot};
    use rusqlite::types::ValueRef;
    use rusqlite::Connection;

    /// 表を丸ごと読んで、列名 → 値のオブジェクトの列にする (D1 の `SELECT *` と同じ形)。
    fn dump(conn: &Connection, table: &str) -> Vec<Value> {
        let mut stmt = conn.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
        let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        stmt.query_map([], |r| {
            let mut o = serde_json::Map::new();
            for (i, n) in names.iter().enumerate() {
                let v = match r.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(x) => Value::from(x),
                    ValueRef::Real(x) => Value::from(x),
                    ValueRef::Text(t) => Value::from(String::from_utf8_lossy(t).to_string()),
                    ValueRef::Blob(_) => Value::Null,
                };
                o.insert(n.clone(), v);
            }
            Ok(Value::Object(o))
        })
        .unwrap()
        .filter_map(Result::ok)
        .collect()
    }

    fn table_exists(conn: &Connection, t: &str) -> bool {
        conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1", [t], |r| r.get::<_, i64>(0)).unwrap() > 0
    }

    /// 項目・髪型の行を足した実データの DB から、(a) 通常のローダ (b) この経路 の両方で組んだ Snapshot が、
    /// 全アイドルで同じ項目の束を返す。行を 1000 件ずつに分けて渡し、順序もわざと入れ替える。
    #[test]
    fn the_mirror_path_builds_the_same_facts_as_the_sqlite_loader() {
        let conn = bundle_conn();
        let mut loader = MirrorLoader::new();
        for table in LOADABLE_TABLES {
            if !table_exists(&conn, table) {
                continue; // 昇格前の idol_facets など
            }
            let mut rows = dump(&conn, table);
            rows.reverse();
            for chunk in rows.chunks(1000) {
                loader.add_rows(table, chunk.to_vec()).unwrap();
            }
        }
        let mirrored = loader.finish();
        let original = bundle_snapshot();
        assert_eq!(mirrored.idols.len(), original.idols.len());
        assert_eq!(mirrored.songs.len(), original.songs.len());
        assert_eq!(mirrored.shows.len(), original.shows.len());
        assert_eq!(mirrored.setlist_items.len(), original.setlist_items.len());
        let persona = PersonaInput::default();
        let mut compared = 0;
        for idol in &original.idols {
            let a = build_idol_facts(original, &idol.id, &persona, &[]);
            let b = build_idol_facts(&mirrored, &idol.id, &persona, &[]);
            assert_eq!(a, b, "{} の項目の束が違う", idol.id);
            compared += 1;
        }
        assert!(compared > 300, "{compared}");
    }


    /// 項目と髪型 (まだ同梱のマスタに表が無い) も、通常のローダと同じ束になる。複数値は '、' 区切りの列から割る。
    #[test]
    fn facets_and_hairstyles_load_like_the_sqlite_loader() {
        use crate::domain::snapshot_build::build;
        use crate::outbound::sqlite_loader::load_raw_tables;
        let idol = "765as_天海春香";
        let mut raw = load_raw_tables(crate::test_support::bundle_path()).unwrap();
        raw.idol_facets = vec![
            IdolFacetRow { id: "f1".into(), idol_id: idol.into(), facet: "dialect".into(), value: "関西弁".into(), origin: "official".into(), source_note: Some("出典".into()), sort_order: 0 },
            IdolFacetRow { id: "f2".into(), idol_id: idol.into(), facet: "traits".into(), value: "明るい".into(), origin: "promoted".into(), source_note: None, sort_order: 1 },
        ];
        raw.idol_hairstyles = vec![IdolHairstyleRow {
            id: "h1".into(),
            idol_id: idol.into(),
            label: "基本".into(),
            is_main: true,
            hair_color: Some("茶".into()),
            hair_length: Some("ロング".into()),
            styles: vec!["ウェーブ".into(), "アホ毛".into()],
            accessories: vec!["リボン".into()],
            origin: "official".into(),
            ..Default::default()
        }];
        let expected = build(raw);

        let conn = bundle_conn();
        let mut loader = MirrorLoader::new();
        for table in ["brands", "idols", "idol_voice_actors", "units", "unit_members", "songs", "song_artists", "events", "shows", "show_cast", "setlist_items", "setlist_performers"] {
            loader.add_rows(table, dump(&conn, table)).unwrap();
        }
        loader
            .add_rows(
                "idol_facets",
                vec![
                    serde_json::json!({"id":"f2","idol_id":idol,"facet":"traits","value":"明るい","origin":"promoted","source_note":null,"sort_order":1}),
                    serde_json::json!({"id":"f1","idol_id":idol,"facet":"dialect","value":"関西弁","origin":"official","source_note":"出典","sort_order":0}),
                ],
            )
            .unwrap();
        loader
            .add_rows(
                "idol_hairstyles",
                vec![serde_json::json!({"id":"h1","idol_id":idol,"label":"基本","is_main":1,"hair_color":"茶","hair_color_secondary":null,"hair_length":"ロング","styles":"ウェーブ、アホ毛","bangs":null,"accessories":"リボン","origin":"official","source_note":null,"sort_order":0})],
            )
            .unwrap();
        let got = loader.finish();
        let persona = PersonaInput::default();
        let a = build_idol_facts(&expected, idol, &persona, &[]).unwrap();
        let b = build_idol_facts(&got, idol, &persona, &[]).unwrap();
        assert_eq!(a, b);
        assert_eq!(b["categories"]["personality"]["dialect"]["value"], "関西弁");
        assert_eq!(b["categories"]["appearance"]["hair_style"]["values"].as_array().unwrap().len(), 2);
    }

    /// 履歴の表が無くても、廃止予定の `voice_actors` の列が現在の声優になる。履歴の行があればそちらが勝つ。
    #[test]
    fn the_legacy_voice_actor_column_stands_in_until_history_rows_exist() {
        let idols = |extra: Value| {
            vec![
                serde_json::json!({"id":"a","brand_id":"b","name":"A","sort_order":1,"is_external":0,"voice_actors":"声優 A"}),
                serde_json::json!({"id":"c","brand_id":"b","name":"C","sort_order":2,"is_external":0,"voice_actors":"声優 C"}),
                extra,
            ]
        };
        let mut l = MirrorLoader::new();
        l.add_rows("idols", idols(serde_json::json!({"id":"d","brand_id":"b","name":"D","sort_order":3,"is_external":0,"voice_actors":null}))).unwrap();
        l.add_rows("idol_voice_actors", vec![serde_json::json!({"id":"h1","idol_id":"c","name":"履歴 C","valid_from":"2020-01-01","valid_to":null})]).unwrap();
        let snap = l.finish();
        let name = |id: &str| snap.current_voice_actor(snap.idol_index_by_id[id]).map(|v| v.name.clone());
        assert_eq!(name("a").as_deref(), Some("声優 A"));
        assert_eq!(name("c").as_deref(), Some("履歴 C"));
        assert_eq!(name("d"), None);
    }

    #[test]
    fn rows_without_a_parent_are_skipped_like_the_sqlite_loader() {
        let mut l = MirrorLoader::new();
        l.add_rows("shows", vec![serde_json::json!({"id":"s1","event_id":"gone","name":"n","date":"2026-01-01","sort_order":1})]).unwrap();
        l.add_rows("setlist_items", vec![serde_json::json!({"id":"a","show_id":"s1","song_id":"nope","position":1})]).unwrap();
        let snap = l.finish();
        assert!(snap.shows.is_empty() && snap.setlist_items.is_empty());
    }

    #[test]
    fn unsupported_tables_and_broken_rows_are_errors() {
        let mut l = MirrorLoader::new();
        assert!(l.add_rows("costumes", vec![]).unwrap_err().contains("costumes"));
        let e = l.add_rows("idols", vec![serde_json::json!({"id": "i"})]).unwrap_err();
        assert!(e.contains("idols") && e.contains("0 行目"), "{e}");
        assert!(l.add_rows("song_artists", vec![serde_json::json!({"song_id": "x"})]).is_err());
        assert!(MirrorLoader::supports("songs") && !MirrorLoader::supports("creators"));
    }
}
