//! アイドル詳細のプロフィールに出す「見た目」と「話し方」の射影。
//!
//! - 見た目: 髪型 1 つぶん (`idol_hairstyles`) を 1 版として、髪色の見本と項目 (長さ・髪型・前髪・髪飾り) に、
//!   髪型によらない項目 (`idol_facets` の眼鏡・目の色) を添える。値のある項目だけ。版は main を先頭に `sort_order` 順。
//! - 話し方: 一人称 (場面つき)・プロデューサーの呼び方・性格と話し方の要約 ([`crate::domain::personas`])。
//!
//! 画面は文言と描画だけを持つ。髪色の見本の色・項目の並び・どの項目が「同じ髪型の一覧」へ行けるかはここで決める。

use crate::domain::call_sheet::{idol_call_summary, IdolCallSummary};
use crate::domain::idol_facets::{EYE_COLORS, HAIR_COLORS};
use crate::domain::personas::{self, Form};
use crate::domain::snapshot::{IdolHairstyleRow, Snapshot};

/// 髪色の見本の色 (フラットな 1 色)。語彙 [`HAIR_COLORS`] の各色に 1 つ。語彙に無い色は `None`。
///
/// 絵の髪の色ではなく「その色名の代表色」。ライトでもダークでも面 (`DS.surface`) の上で読める明るさにしてある。
pub fn hair_color_hex(name: &str) -> Option<&'static str> {
    Some(match name {
        "黒" => "#2B2730",
        "茶" => "#8A5A3B",
        "金" => "#E3B94F",
        "白・銀" => "#D9D6DF",
        "赤" => "#C8323C",
        "オレンジ" => "#EE8A3A",
        "ピンク" => "#F09BBF",
        "緑" => "#4E9A5B",
        "青" => "#3A63B8",
        "水色" => "#86C8E8",
        "紫" => "#8E5CC0",
        _ => return None,
    })
}

/// 髪色の見本 1 色。
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct IdolLookSwatch {
    /// 色名 (「黒」「白・銀」)。読み上げにも使う。
    pub name: String,
    /// `#RRGGBB`。
    pub hex: String,
}

/// 見た目の項目の種類。画面はこれで押せるかどうか・行き先を決める。
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdolLookItemKind {
    HairLength,
    /// 髪型 (ツインテール・ポニーテール…)。値の 1 つ 1 つが「同じ髪型のアイドル」の一覧へ行ける。
    HairStyle,
    Bangs,
    HairAccessory,
    Glasses,
    EyeColor,
}

/// 見た目の項目 1 つ (「長さ: ロング」)。
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct IdolLookItem {
    pub kind: IdolLookItemKind,
    pub label: String,
    /// 値。複数値の項目 (髪型・髪飾り) は複数。空の項目は作らない。
    pub values: Vec<String>,
    /// 値を押すと、その値で絞ったアイドルの一覧へ行けるか (髪型だけ)。
    pub opens_idol_list: bool,
}

/// 見た目の版 1 つ (髪型 1 つぶん)。
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct IdolLookVersion {
    /// 版の名前 (「基本」「セカンドヘア」「女装時」)。髪型の無い子は空文字。
    pub label: String,
    pub is_main: bool,
    /// 髪色の見本。1 色か 2 色 (2 色目があれば左右に硬く分ける)。髪色が無ければ空。
    pub swatches: Vec<IdolLookSwatch>,
    /// 見本の下に書く髪色 (「髪色 黒」「髪色 黒／金」)。髪色が無ければ空文字。
    pub swatch_caption: String,
    pub items: Vec<IdolLookItem>,
}

/// アイドルの見た目。版が空なら区画ごと出さない。
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq, Default)]
pub struct IdolLook {
    pub versions: Vec<IdolLookVersion>,
}

/// 見た目を組む。髪型も眼鏡・目の色も無い子は版が空。
pub fn idol_look(snap: &Snapshot, idol_id: &str) -> IdolLook {
    let Some(&ii) = snap.idol_index_by_id.get(idol_id) else { return IdolLook::default() };
    let facet_values = |facet: &str| -> Vec<String> {
        snap.idol_facets_by_idol
            .get(ii as usize)
            .into_iter()
            .flatten()
            .map(|&fi| &snap.idol_facets[fi as usize])
            .filter(|f| f.facet == facet)
            .map(|f| f.value.trim().to_string())
            .filter(|v| !v.is_empty())
            .collect()
    };
    // 髪型によらない項目。眼鏡は「かけている」ときだけ出す (かけていないのは書くほどのことでない)。
    let mut shared: Vec<IdolLookItem> = Vec::new();
    if facet_values("glasses").iter().any(|v| matches!(v.as_str(), "true" | "1")) {
        shared.push(item(IdolLookItemKind::Glasses, "眼鏡", vec!["かけている".to_string()]));
    }
    let eyes: Vec<String> = facet_values("eye_color").into_iter().filter(|v| EYE_COLORS.contains(&v.as_str())).take(1).collect();
    if !eyes.is_empty() {
        shared.push(item(IdolLookItemKind::EyeColor, "目の色", eyes));
    }

    let mut rows: Vec<&IdolHairstyleRow> = snap
        .idol_hairstyles_by_idol
        .get(ii as usize)
        .into_iter()
        .flatten()
        .map(|&hi| &snap.idol_hairstyles[hi as usize])
        .collect();
    // main を先頭に、残りは sort_order 順 (索引の並び = (sort_order, id))。main が無ければ並びのまま。
    rows.sort_by_key(|r| !r.is_main);

    let mut versions: Vec<IdolLookVersion> = rows.into_iter().map(|r| version_of(r, &shared)).collect();
    if versions.is_empty() && !shared.is_empty() {
        versions.push(IdolLookVersion {
            label: String::new(),
            is_main: true,
            swatches: Vec::new(),
            swatch_caption: String::new(),
            items: shared,
        });
    }
    IdolLook { versions }
}

fn item(kind: IdolLookItemKind, label: &str, values: Vec<String>) -> IdolLookItem {
    IdolLookItem { kind, label: label.to_string(), values, opens_idol_list: kind == IdolLookItemKind::HairStyle }
}

fn version_of(r: &IdolHairstyleRow, shared: &[IdolLookItem]) -> IdolLookVersion {
    let colors: Vec<&str> = [r.hair_color.as_deref(), r.hair_color_secondary.as_deref()]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .collect();
    let swatches: Vec<IdolLookSwatch> = colors
        .iter()
        .filter(|c| HAIR_COLORS.contains(c))
        .filter_map(|c| hair_color_hex(c).map(|hex| IdolLookSwatch { name: c.to_string(), hex: hex.to_string() }))
        .collect();
    let swatch_caption = if colors.is_empty() { String::new() } else { format!("髪色 {}", colors.join("／")) };

    let mut items = Vec::new();
    let one = |v: &Option<String>| -> Vec<String> {
        v.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| vec![s.to_string()]).unwrap_or_default()
    };
    let many = |v: &[String]| -> Vec<String> { v.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect() };
    for (kind, label, values) in [
        (IdolLookItemKind::HairLength, "長さ", one(&r.hair_length)),
        (IdolLookItemKind::HairStyle, "髪型", many(&r.styles)),
        (IdolLookItemKind::Bangs, "前髪", one(&r.bangs)),
        (IdolLookItemKind::HairAccessory, "髪飾り", many(&r.accessories)),
    ] {
        if !values.is_empty() {
            items.push(item(kind, label, values));
        }
    }
    items.extend(shared.iter().cloned());
    IdolLookVersion { label: r.label.trim().to_string(), is_main: r.is_main, swatches, swatch_caption, items }
}

// ---------------------------------------------------------------------------
// 話し方
// ---------------------------------------------------------------------------

/// 呼び方 1 つ (場面つき)。
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct IdolSpeechForm {
    pub form: String,
    /// 使う場面 (「普段」「真面目なとき」)。分かれていなければ `None`。
    pub scene: Option<String>,
}

impl From<Form> for IdolSpeechForm {
    fn from(f: Form) -> Self {
        Self { form: f.form, scene: f.scene }
    }
}

/// 話し方の掲示板。
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct IdolSpeech {
    pub first_person: Vec<IdolSpeechForm>,
    pub producer_call: Vec<IdolSpeechForm>,
    /// 性格と話し方の要約 (掲示板の下に添える文)。
    pub tone_notes: Option<String>,
}

/// 話し方。一人称もプロデューサーの呼び方も要約も無い子は `None`。
pub fn idol_speech(idol_id: &str) -> Option<IdolSpeech> {
    let first_person: Vec<IdolSpeechForm> = personas::first_person_forms(idol_id).into_iter().map(Into::into).collect();
    let producer_call: Vec<IdolSpeechForm> = personas::producer_call_forms(idol_id).into_iter().map(Into::into).collect();
    let tone_notes = personas::tone_notes(idol_id);
    if first_person.is_empty() && producer_call.is_empty() && tone_notes.is_none() {
        return None;
    }
    Some(IdolSpeech { first_person, producer_call, tone_notes })
}

/// プロフィールのタブで所属ユニットより上に置く 3 つ (見た目・話し方・呼び方)。タブを開いたとき 1 回で引く。
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct IdolProfileExtras {
    pub look: IdolLook,
    pub speech: Option<IdolSpeech>,
    pub calls: IdolCallSummary,
}

pub fn idol_profile_extras(snap: &Snapshot, idol_id: &str) -> IdolProfileExtras {
    IdolProfileExtras { look: idol_look(snap, idol_id), speech: idol_speech(idol_id), calls: idol_call_summary(snap, idol_id) }
}

/// 同じ髪型 (`style`) の版を 1 つでも持つアイドルの添字。公式順 (`idol_order`)。
pub fn idol_indices_with_hair_style(snap: &Snapshot, style: &str) -> Vec<u32> {
    snap.idol_order
        .iter()
        .copied()
        .filter(|&ii| {
            snap.idol_hairstyles_by_idol
                .get(ii as usize)
                .into_iter()
                .flatten()
                .any(|&hi| snap.idol_hairstyles[hi as usize].styles.iter().any(|s| s == style))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::bundle_snapshot;

    #[test]
    fn 語彙の髪色にはすべて見本の色がある() {
        for c in HAIR_COLORS {
            let hex = hair_color_hex(c).unwrap_or_else(|| panic!("{c} の見本が無い"));
            assert!(hex.len() == 7 && hex.starts_with('#'), "{c}: {hex}");
        }
        assert_eq!(hair_color_hex("虹"), None);
    }

    fn row(label: &str, is_main: bool, sort_order: i64) -> IdolHairstyleRow {
        IdolHairstyleRow {
            id: format!("ih_{label}"),
            idol_id: "x".into(),
            label: label.into(),
            is_main,
            hair_color: Some("黒".into()),
            hair_color_secondary: Some("金".into()),
            hair_length: Some("ロング".into()),
            styles: vec!["ツインテール".into(), "縦ロール".into()],
            bangs: None,
            accessories: vec![],
            origin: "official".into(),
            source_note: None,
            sort_order,
        }
    }

    #[test]
    fn 版は髪色の見本と値のある項目だけを持つ() {
        let v = version_of(&row("基本", true, 0), &[]);
        assert_eq!(v.swatches.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["黒", "金"]);
        assert_eq!(v.swatch_caption, "髪色 黒／金");
        let kinds: Vec<IdolLookItemKind> = v.items.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, [IdolLookItemKind::HairLength, IdolLookItemKind::HairStyle]);
        assert!(v.items[1].opens_idol_list && !v.items[0].opens_idol_list);
        assert_eq!(v.items[1].values, ["ツインテール", "縦ロール"]);
    }

    #[test]
    fn 実データの髪型が版になりmainが先頭に来る() {
        let snap = bundle_snapshot();
        let with_many = snap
            .idol_hairstyles_by_idol
            .iter()
            .position(|v| v.len() >= 2)
            .expect("髪型が 2 つ以上ある子がいる");
        let look = idol_look(snap, &snap.idols[with_many].id);
        assert!(look.versions.len() >= 2);
        assert!(look.versions[0].is_main);
        assert!(look.versions.iter().skip(1).all(|v| !v.is_main));

        let main_only = snap.idol_hairstyles_by_idol.iter().position(|v| v.len() == 1).unwrap();
        let look = idol_look(snap, &snap.idols[main_only].id);
        assert_eq!(look.versions.len(), 1);
        assert!(look.versions[0].items.iter().all(|i| !i.values.is_empty()));
    }

    #[test]
    fn 何も無い子は版が空() {
        let snap = bundle_snapshot();
        let none = snap
            .idol_hairstyles_by_idol
            .iter()
            .zip(&snap.idol_facets_by_idol)
            .position(|(h, f)| h.is_empty() && f.is_empty())
            .unwrap();
        assert!(idol_look(snap, &snap.idols[none].id).versions.is_empty());
        assert!(idol_look(snap, "no-such-idol").versions.is_empty());
    }

    #[test]
    fn 話し方は一人称を場面つきで返す() {
        let s = idol_speech("sc_三峰結華").unwrap();
        assert_eq!(s.first_person[0], IdolSpeechForm { form: "三峰".into(), scene: Some("普段".into()) });
        assert_eq!(s.producer_call[0].form, "Pたん");
        assert!(s.tone_notes.unwrap().contains("ニックネーム"));
        assert_eq!(idol_speech("no-such-idol"), None);
    }

    #[test]
    fn 同じ髪型の子を公式順で引く() {
        let snap = bundle_snapshot();
        let hits = idol_indices_with_hair_style(snap, "ツインテール");
        assert!(hits.len() >= 3, "{}", hits.len());
        let orders: Vec<i64> = hits.iter().map(|&i| snap.idols[i as usize].official_order_key()).collect();
        assert!(orders.windows(2).all(|w| w[0] <= w[1]));
        assert!(idol_indices_with_hair_style(snap, "存在しない髪型").is_empty());
    }
}
