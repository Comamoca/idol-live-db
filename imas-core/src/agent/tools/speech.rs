//! アイドルの話し方 (一人称・プロデューサーの呼び方・語尾・口癖・呼称) を LLM 向けの形にする。
//!
//! データ (`data/personas/`) を読むのは [`crate::domain::personas`]。ここは公開 API と項目 (`/facts`) の
//! 話し方に整えるだけ。データの無いアイドルは何も返さない。

use serde_json::Value;

use crate::domain::personas::{persona, render};
pub use crate::domain::personas::{call_name, call_names_from, catchphrases, producer_call};

/// 公開 API 向けの話し方 (一人称・プロデューサーの呼び方・語尾・口癖・呼称)。
///
/// 話し方のデータを `render` で項目ごとに 1 行にしたもの。
/// 出典 URL (`sources`) は載せない。データの無い子は `None`。
/// `names` は呼称の相手 id → 名前の引き (マスタに居ない相手は落とす)。
pub fn speech_profile(idol_id: &str, name_of: impl Fn(&str) -> Option<String>) -> Option<Value> {
    let persona = persona(idol_id);
    let mut o = serde_json::Map::new();
    let mut put = |key: &str, v: Option<String>| {
        if let Some(v) = v {
            o.insert(key.to_string(), Value::String(v));
        }
    };
    put("first_person", persona.and_then(|p| render(&p["first_person"])));
    put("producer_call", producer_call(idol_id));
    put("politeness", persona.and_then(|p| render(&p["politeness"])));
    put("tone_notes", persona.and_then(|p| render(&p["tone_notes"])));
    let endings: Vec<Value> = persona
        .and_then(|p| p["endings"].as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| Value::String(s.to_string()))).collect())
        .unwrap_or_default();
    if !endings.is_empty() {
        o.insert("endings".into(), Value::Array(endings));
    }
    let phrases: Vec<Value> = catchphrases(idol_id).into_iter().map(Value::String).collect();
    if !phrases.is_empty() {
        o.insert("catchphrases".into(), Value::Array(phrases));
    }
    let calls: Vec<Value> = call_names_from(idol_id)
        .into_iter()
        .filter_map(|(to, called)| {
            let name = name_of(&to)?;
            Some(serde_json::json!({ "id": to, "name": name, "called": called }))
        })
        .collect();
    if !calls.is_empty() {
        o.insert("call_names".into(), Value::Array(calls));
    }
    (!o.is_empty()).then(|| Value::Object(o))
}

/// 項目 (`/facts`) の personality に渡す話し方。`render` の「A／B（場面）」を項目ごとの値に割る。
/// データの無い子は空の入力。
pub fn persona_input(idol_id: &str) -> crate::domain::idol_facets::PersonaInput {
    let persona = persona(idol_id);
    let list = |v: Option<String>| -> Vec<String> {
        v.map(|t| t.split('／').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()).unwrap_or_default()
    };
    crate::domain::idol_facets::PersonaInput {
        first_person: list(persona.and_then(|p| render(&p["first_person"]))),
        producer_call: list(producer_call(idol_id)),
        catchphrases: catchphrases(idol_id),
        sentence_endings: persona
            .and_then(|p| p["endings"].as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        politeness: persona.and_then(|p| render(&p["politeness"])),
        tone_notes: persona.and_then(|p| render(&p["tone_notes"])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 話し方ファイルのある子は一人称と呼び方が出る() {
        let profile = speech_profile("765as_四条貴音", |_| None).unwrap();
        assert!(profile["first_person"].as_str().unwrap().contains("わたくし"), "{profile}");
        assert!(profile["producer_call"].as_str().unwrap().contains("あなた様"), "{profile}");
    }

    #[test]
    fn データの無い子は何も返さない() {
        assert_eq!(speech_profile("no-such-idol", |_| None), None);
    }
}
