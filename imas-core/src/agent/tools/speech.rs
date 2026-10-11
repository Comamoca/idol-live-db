//! アイドルの話し方 (一人称・プロデューサーの呼び方・語尾・口癖・呼称) を LLM 向けの形にする。
//!
//! データ (`data/personas/`) を読むのは [`crate::domain::personas`]。ここはトーク・タイムラインの指示文と
//! 公開 API の話し方に整えるだけ。データの無いアイドルは何も返さない (指示文は今まで通りモデルの知識に任せる)。

use serde_json::Value;

use crate::domain::personas::{persona, render};
pub use crate::domain::personas::{call_name, call_names_from, catchphrases, producer_call};

/// 公開 API 向けの話し方 (一人称・プロデューサーの呼び方・語尾・口癖・呼称)。
///
/// `speech_block` (トークの指示文) と同じデータを同じ `render` で 1 行にしたもの。
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

/// 照合用に、記号・空白・伸ばし棒の揺れを落とす。
fn fold(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_whitespace() && !"♪♡☆★！!？?、。,.…～〜ー-「」『』()（）".contains(*c))
        .collect()
}

/// `texts` (その子の直近の発言) に出てきた口癖。
///
/// 口癖は「○○ですよ、○○！」のように差し込み部分を含むので、○ / 〇 / … で切った
/// 2 文字以上の断片がすべて含まれていれば「使った」とみなす。
pub fn used_catchphrases(idol_id: &str, texts: &[String]) -> Vec<String> {
    let folded: Vec<String> = texts.iter().map(|t| fold(t)).collect();
    catchphrases(idol_id)
        .into_iter()
        .filter(|phrase| {
            let parts: Vec<String> = phrase
                .split(['○', '〇', '…', '◯'])
                .map(fold)
                .filter(|p| p.chars().count() >= 2)
                .collect();
            !parts.is_empty() && folded.iter().any(|t| parts.iter().all(|p| t.contains(p.as_str())))
        })
        .collect()
}

/// 語尾・口癖をどう扱うか (トークとタイムラインで共通の言い回し)。
pub const CATCHPHRASE_RULE: &str = "語尾・口癖は参考。毎回は使わない。ここぞという時だけ、数メッセージに 1 回程度にする。";

/// 指示文に入れる「話し方」。データが何も無ければ None。
///
/// 必ず守らせるのは一人称・プロデューサーの呼び方・仲間の呼び方だけ。性格と話し方の要約
/// (`tone_notes`) を主に渡し、語尾・口癖は参考として添える (連打すると不自然になるため)。
/// `others` は呼び方を添える相手 (id, 名前)、`avoid` は直近で使ったので今回は使わない口癖。
pub fn speech_block(idol_id: &str, others: &[(String, String)], avoid: &[String]) -> Option<String> {
    let persona = persona(idol_id);
    let mut must: Vec<String> = Vec::new();
    if let Some(v) = persona.and_then(|p| render(&p["first_person"])) {
        must.push(format!("一人称: {v}"));
    }
    if let Some(v) = producer_call(idol_id) {
        must.push(format!("プロデューサーの呼び方: {v}"));
    }
    let calls: Vec<String> = others
        .iter()
        .filter(|(id, _)| id != idol_id)
        .filter_map(|(id, name)| call_name(idol_id, id).map(|c| format!("{name}→「{c}」")))
        .collect();
    if !calls.is_empty() {
        must.push(format!("仲間の呼び方: {}", calls.join("、")));
    }

    let mut reference: Vec<String> = Vec::new();
    if let Some(p) = persona {
        for (key, label) in [("tone_notes", "性格と話し方"), ("politeness", "敬語の使い方")] {
            if let Some(v) = render(&p[key]) {
                reference.push(format!("{label}: {v}"));
            }
        }
        let flavor: Vec<String> = [&p["endings"], &p["catchphrases"]].into_iter().filter_map(render).collect();
        if !flavor.is_empty() {
            reference.push(format!("語尾・口癖の例: {} ({CATCHPHRASE_RULE})", flavor.join("／")));
        }
    }
    if !avoid.is_empty() {
        let list: Vec<String> = avoid.iter().map(|a| format!("『{a}』")).collect();
        reference.push(format!("直近で {} を使ったので、今回は使わない", list.join("・")));
    }

    if must.is_empty() && reference.is_empty() {
        return None;
    }
    let mut out = String::new();
    if !must.is_empty() {
        out.push_str("必ず守る (このとおりに書く):\n");
        for line in &must {
            out.push_str(&format!("- {line}\n"));
        }
    }
    if !reference.is_empty() {
        out.push_str("参考 (雰囲気をつかむためのもの。なぞらない):\n");
        for line in &reference {
            out.push_str(&format!("- {line}\n"));
        }
    }
    Some(out.trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 話し方ファイルのある子は一人称と呼び方が出る() {
        let block = speech_block("765as_四条貴音", &[], &[]).unwrap();
        assert!(block.contains("一人称: わたくし"), "{block}");
        assert!(block.contains("あなた様"), "{block}");
    }

    #[test]
    fn 呼称データにある仲間の呼び方が出る() {
        // 呼称データには自称 (自分を呼ぶ行) もあるので、相手が自分でない組を決まった順で選ぶ。
        let (from, to) = crate::domain::personas::call_keys()
            .filter(|(f, t)| f.starts_with("ml_") && t.starts_with("ml_") && f != t)
            .min()
            .cloned()
            .unwrap();
        let block = speech_block(&from, &[(to.clone(), "相手".into())], &[]).unwrap();
        assert!(block.contains("相手→「"), "{block}");
    }

    #[test]
    fn 口癖は参考扱いで毎回使わないと書く() {
        let block = speech_block("765as_天海春香", &[], &[]).unwrap();
        let (must, reference) = block.split_once("参考").unwrap();
        assert!(must.contains("一人称"), "{block}");
        assert!(!must.contains("口癖"), "口癖を「必ず守る」に入れない: {block}");
        assert!(reference.contains("毎回は使わない"), "{block}");
        assert!(reference.contains("性格と話し方"), "{block}");
    }

    #[test]
    fn 差し込みのある口癖も直近の発言から見つけて避けさせる() {
        // 天海春香の口癖「○○ですよ、○○！」は差し込み部分を除いた「ですよ」で当てる。
        let recent = vec!["プロデューサーさん、ライブですよ、ライブ！".to_string()];
        let used = used_catchphrases("765as_天海春香", &recent);
        assert_eq!(used, vec!["○○ですよ、○○！".to_string()]);
        let block = speech_block("765as_天海春香", &[], &used).unwrap();
        assert!(block.contains("直近で 『○○ですよ、○○！』 を使ったので、今回は使わない"), "{block}");
        assert!(used_catchphrases("765as_天海春香", &["こんにちは".to_string()]).is_empty());
    }

    #[test]
    fn データの無い子は何も返さない() {
        assert_eq!(speech_block("no-such-idol", &[], &[]), None);
    }
}
