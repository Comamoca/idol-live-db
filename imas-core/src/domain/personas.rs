//! アイドルの話し方 (一人称・プロデューサーの呼び方・語尾・口癖・性格と話し方の要約) と呼称 (誰が誰をどう呼ぶか) を
//! アプリに同梱して引く。DB や CloudKit には入れていない。
//!
//! - `data/personas/<brand>.json`: 出典付きで 1 人ずつ確かめた話し方。ブランドごとに足していく。
//!   一人称・呼び方は文字列 / 文字列の配列 / `{form, scene}` の配列のどれでも来る
//!   (場面で使い分ける子がいるため)。
//! - `data/personas/call_names.json`: im@sparql (MIT License, © 2017 IM@Study,
//!   https://github.com/crssnky/imasparql) の呼称データと、ニコニコ大百科の呼称表の行。誰が誰をどう呼ぶか。
//!   行の `source` で出どころを分けるが、ここでは使わず、同じ from/to の呼び方は重複なく束ねる。
//!   `to` が `producer` の行はプロデューサーの呼び方、`from` と同じ id の行は一人称。
//!
//! 読む側はアイドル詳細の「話し方」「呼び方」・ユニットの呼称表 ([`crate::domain::idol_appearance`]) と、
//! LLM 向けの指示文 (`agent::tools::speech`)。データの無いアイドルは何も返さない。

use serde_json::Value;
use std::collections::HashMap;
use std::sync::OnceLock;

/// ブランドごとの話し方ファイル。ブランドを足したらここに 1 行足す。
const PERSONA_FILES: &[&str] = &[
    include_str!("../../../data/personas/765as.json"),
    include_str!("../../../data/personas/ml.json"),
    include_str!("../../../data/personas/cg.json"),
    include_str!("../../../data/personas/sc.json"),
    include_str!("../../../data/personas/gakuen.json"),
    include_str!("../../../data/personas/sidem.json"),
    include_str!("../../../data/personas/876.json"),
    include_str!("../../../data/personas/961.json"),
];
const CALL_NAMES: &str = include_str!("../../../data/personas/call_names.json");

/// 呼称データでプロデューサーを表す `to`。
pub const PRODUCER: &str = "producer";

struct PersonaData {
    personas: HashMap<String, Value>,
    /// (呼ぶ側, 呼ばれる側 or "producer") → 呼び方 (複数あれば出てきた順)。
    calls: HashMap<(String, String), Vec<String>>,
}

fn data() -> &'static PersonaData {
    static DATA: OnceLock<PersonaData> = OnceLock::new();
    DATA.get_or_init(|| {
        let mut personas = HashMap::new();
        for file in PERSONA_FILES {
            let rows: Vec<Value> = serde_json::from_str(file).expect("data/personas の JSON が壊れている");
            for row in rows {
                if let Some(id) = row["idol_id"].as_str() {
                    personas.insert(id.to_string(), row.clone());
                }
            }
        }
        let mut calls: HashMap<(String, String), Vec<String>> = HashMap::new();
        let rows: Vec<Value> = serde_json::from_str(CALL_NAMES).expect("call_names.json が壊れている");
        for row in rows {
            if let (Some(from), Some(to), Some(called)) = (row["from"].as_str(), row["to"].as_str(), row["called"].as_str()) {
                let list = calls.entry((from.to_string(), to.to_string())).or_default();
                if !list.iter().any(|c| c == called) {
                    list.push(called.to_string());
                }
            }
        }
        PersonaData { personas, calls }
    })
}

/// 話し方ファイルのその子の行 (無ければ `None`)。
pub fn persona(idol_id: &str) -> Option<&'static Value> {
    data().personas.get(idol_id)
}

/// 話し方ファイルを持つアイドルの id (順不同)。
pub fn persona_ids() -> impl Iterator<Item = &'static String> {
    data().personas.keys()
}

/// 呼称データの (呼ぶ側, 呼ばれる側) の組 (順不同)。呼ばれる側は [`PRODUCER`] のこともある。
pub fn call_keys() -> impl Iterator<Item = &'static (String, String)> {
    data().calls.keys()
}

/// 呼び方 1 つ。場面で使い分ける子は場面つき (「三峰」(普段) / 「私」(真面目なとき))。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Form {
    pub form: String,
    pub scene: Option<String>,
}

/// 文字列 / 配列 / `{form, scene}` の配列を、呼び方の並びにする (空の値は落とす)。
pub fn forms(value: &Value) -> Vec<Form> {
    match value {
        Value::String(s) => {
            let s = s.trim();
            if s.is_empty() { Vec::new() } else { vec![Form { form: s.to_string(), scene: None }] }
        }
        Value::Array(items) => items
            .iter()
            .flat_map(|item| match item {
                Value::Object(o) => o
                    .get("form")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|f| !f.is_empty())
                    .map(|form| Form {
                        form: form.to_string(),
                        scene: o.get("scene").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from),
                    })
                    .into_iter()
                    .collect(),
                other => forms(other),
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// 呼び方の並びを「A／B（場面）」の 1 行にする。空なら `None`。
pub fn join_forms(forms: &[Form]) -> Option<String> {
    let text = forms
        .iter()
        .map(|f| match &f.scene {
            Some(scene) => format!("{}（{scene}）", f.form),
            None => f.form.clone(),
        })
        .collect::<Vec<_>>()
        .join("／");
    (!text.is_empty()).then_some(text)
}

/// 文字列 / 配列 / `{form, scene}` を「A／B（場面）」の 1 行にする。
pub fn render(value: &Value) -> Option<String> {
    join_forms(&forms(value))
}

/// 一人称。話し方ファイルを優先し、無ければ呼称データの自分を呼ぶ行。
pub fn first_person_forms(idol_id: &str) -> Vec<Form> {
    let from_persona = persona(idol_id).map(|p| forms(&p["first_person"])).unwrap_or_default();
    if !from_persona.is_empty() {
        return from_persona;
    }
    plain_forms(data().calls.get(&(idol_id.to_string(), idol_id.to_string())))
}

/// プロデューサーの呼び方。話し方ファイルを優先し、無ければ呼称データ。
pub fn producer_call_forms(idol_id: &str) -> Vec<Form> {
    let from_persona = persona(idol_id).map(|p| forms(&p["producer_call"])).unwrap_or_default();
    if !from_persona.is_empty() {
        return from_persona;
    }
    plain_forms(data().calls.get(&(idol_id.to_string(), PRODUCER.to_string())))
}

fn plain_forms(calls: Option<&Vec<String>>) -> Vec<Form> {
    calls.map(|c| c.iter().map(|form| Form { form: form.clone(), scene: None }).collect()).unwrap_or_default()
}

/// プロデューサーの呼び方の 1 行。
pub fn producer_call(idol_id: &str) -> Option<String> {
    join_forms(&producer_call_forms(idol_id))
}

/// `from` が `to` をどう呼ぶか (複数あれば「／」で束ねる)。
pub fn call_name(from: &str, to: &str) -> Option<String> {
    data().calls.get(&(from.to_string(), to.to_string())).map(|c| c.join("／"))
}

/// `from` が呼び方を持っている相手 (アイドルだけ。自分は除く) と呼び方。相手の id 順。
pub fn call_names_from(from: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = data()
        .calls
        .iter()
        .filter(|((f, t), _)| f == from && t != PRODUCER && t != from)
        .map(|((_, t), c)| (t.clone(), c.join("／")))
        .collect();
    out.sort();
    out
}

/// `to` を呼び方を持っている相手 (自分は除く) と呼び方。相手の id 順。
pub fn call_names_to(to: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = data()
        .calls
        .iter()
        .filter(|((f, t), _)| t == to && f != to)
        .map(|((f, _), c)| (f.clone(), c.join("／")))
        .collect();
    out.sort();
    out
}

/// その子の口癖・決まり文句 (話し方ファイルの `catchphrases`)。
pub fn catchphrases(idol_id: &str) -> Vec<String> {
    let Some(p) = persona(idol_id) else { return Vec::new() };
    match &p["catchphrases"] {
        Value::String(s) => vec![s.clone()],
        Value::Array(items) => items.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    }
}

/// 性格と話し方の要約 (話し方ファイルの `tone_notes`)。
pub fn tone_notes(idol_id: &str) -> Option<String> {
    persona(idol_id).and_then(|p| render(&p["tone_notes"]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::bundle_snapshot;

    #[test]
    fn 同梱した話し方のidはすべてマスタにいる() {
        let snap = bundle_snapshot();
        for id in persona_ids() {
            assert!(snap.idol_index_by_id.contains_key(id), "{id} がマスタに無い");
        }
        for (from, to) in call_keys() {
            assert!(snap.idol_index_by_id.contains_key(from), "{from} がマスタに無い");
            assert!(to == PRODUCER || snap.idol_index_by_id.contains_key(to), "{to} がマスタに無い");
        }
    }

    /// 呼称表の列ずれで「相手を呼ぶ欄に自分の名前」が入った行を持ち込まない。
    /// 自分の名前 (下の名前) で相手を呼ぶことはまず無いので、そういう行があれば取り込みの誤り。
    #[test]
    fn 相手を自分の下の名前で呼ぶ行は無い() {
        let snap = bundle_snapshot();
        let mut bad = Vec::new();
        for ((from, to), called) in &data().calls {
            if from == to || to == PRODUCER {
                continue;
            }
            let Some(&fi) = snap.idol_index_by_id.get(from) else { continue };
            let Some(given) = snap.idols[fi as usize].given_name.as_deref().filter(|g| !g.is_empty()) else { continue };
            if called.iter().any(|c| c == given) {
                bad.push(format!("{from} → {to}「{given}」"));
            }
        }
        bad.sort();
        assert!(bad.is_empty(), "自分の名前で相手を呼んでいる行: {bad:?}");
    }

    #[test]
    fn 場面つきの一人称と配列の呼び方を1行にする() {
        assert_eq!(render(&serde_json::json!("私")).as_deref(), Some("私"));
        assert_eq!(render(&serde_json::json!(["あなた様", "プロデューサー"])).as_deref(), Some("あなた様／プロデューサー"));
        assert_eq!(
            render(&serde_json::json!([{ "form": "亜美", "scene": "普段" }, { "form": "私" }])).as_deref(),
            Some("亜美（普段）／私")
        );
        assert_eq!(render(&serde_json::json!(null)), None);
        assert_eq!(render(&serde_json::json!("  ")), None);
    }

    #[test]
    fn 場面で使い分ける一人称は場面つきで返す() {
        let fp = first_person_forms("sc_三峰結華");
        assert_eq!(fp[0], Form { form: "三峰".into(), scene: Some("普段".into()) });
        assert_eq!(fp[1].scene.as_deref(), Some("真面目なとき"));
        assert_eq!(join_forms(&producer_call_forms("sc_三峰結華")).as_deref(), Some("Pたん"));
    }

    #[test]
    fn 呼ぶ側と呼ばれる側の両方から引ける() {
        let out = call_names_from("sc_三峰結華");
        assert!(out.iter().any(|(to, c)| to == "sc_田中摩美々" && c == "まみみん"), "{out:?}");
        assert!(out.iter().all(|(to, _)| to != "sc_三峰結華" && to != PRODUCER));
        let inc = call_names_to("sc_三峰結華");
        assert!(inc.iter().any(|(from, _)| from == "sc_田中摩美々"), "{inc:?}");
        assert!(inc.iter().all(|(from, _)| from != "sc_三峰結華"));
    }
}
