//! ブランド内の属性 (`idols.attribute`) と、属性をさらに分けたサブ属性 (`idols.sub_attribute`) の表。
//!
//! **選択肢と表示名はここが唯一の正。** iOS / Android / Web はこの表を FFI / wasm 越しに
//! 受け取って並べるだけにする (以前は各 OS が属性の対応表を書き写していた)。
//!
//! サブ属性は今のところミリオンライブ！ シアターデイズの「BATTLE OF THEATER」の 6 チーム
//! (Princess / Fairy / Angel をそれぞれ 2 つに分けたもの。765AS の 13 人も入る)。
//! 出典: <https://idolmaster-official.jp/news/01_15862>

/// 選択肢 1 件。`value` は DB に入っている値、`label` は画面に出す名前。
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct IdolAttributeOption {
    pub value: String,
    pub label: String,
}

struct AttributeDef {
    brand_ids: &'static [&'static str],
    value: &'static str,
    label: &'static str,
}

struct SubAttributeDef {
    /// このサブ属性が属する属性 (`idols.attribute`)。
    attribute: &'static str,
    value: &'static str,
    label: &'static str,
}

const ML_765: &[&str] = &["ml", "765as"];

/// 並びが画面の並び。
const ATTRIBUTES: &[AttributeDef] = &[
    AttributeDef { brand_ids: &["cg"], value: "cute", label: "キュート" },
    AttributeDef { brand_ids: &["cg"], value: "cool", label: "クール" },
    AttributeDef { brand_ids: &["cg"], value: "passion", label: "パッション" },
    AttributeDef { brand_ids: ML_765, value: "princess", label: "プリンセス" },
    AttributeDef { brand_ids: ML_765, value: "fairy", label: "フェアリー" },
    AttributeDef { brand_ids: ML_765, value: "angel", label: "エンジェル" },
    AttributeDef { brand_ids: &["sidem"], value: "intelli", label: "インテリ" },
    AttributeDef { brand_ids: &["sidem"], value: "physical", label: "フィジカル" },
    AttributeDef { brand_ids: &["sidem"], value: "mental", label: "メンタル" },
    AttributeDef { brand_ids: &["sc"], value: "sol", label: "Sol" },
    AttributeDef { brand_ids: &["sc"], value: "luna", label: "Luna" },
    AttributeDef { brand_ids: &["sc"], value: "stella", label: "Stella" },
];

/// 並びが画面の並び (公式の発表順)。
const SUB_ATTRIBUTES: &[SubAttributeDef] = &[
    SubAttributeDef { attribute: "princess", value: "pretty", label: "プリティ" },
    SubAttributeDef { attribute: "princess", value: "girly", label: "ガーリー" },
    SubAttributeDef { attribute: "fairy", value: "mystical", label: "ミスティカル" },
    SubAttributeDef { attribute: "fairy", value: "breaker", label: "ブレイカー" },
    SubAttributeDef { attribute: "angel", value: "sweetie", label: "スウィーティ" },
    SubAttributeDef { attribute: "angel", value: "charming", label: "チャーミング" },
];

fn option(value: &str, label: &str) -> IdolAttributeOption {
    IdolAttributeOption { value: value.to_string(), label: label.to_string() }
}

/// そのブランドの属性の選択肢。属性で分けないブランドは空。
pub fn attribute_options(brand_id: &str) -> Vec<IdolAttributeOption> {
    ATTRIBUTES
        .iter()
        .filter(|a| a.brand_ids.contains(&brand_id))
        .map(|a| option(a.value, a.label))
        .collect()
}

/// その属性を分けたサブ属性の選択肢。サブ属性の無い属性は空。
pub fn sub_attribute_options(attribute: &str) -> Vec<IdolAttributeOption> {
    SUB_ATTRIBUTES.iter().filter(|t| t.attribute == attribute).map(|t| option(t.value, t.label)).collect()
}

/// サブ属性の値すべて (表の並び)。
pub fn all_sub_attributes() -> Vec<String> {
    SUB_ATTRIBUTES.iter().map(|t| t.value.to_string()).collect()
}

/// 属性の表示名 (`princess` → 「プリンセス」)。表に無い値は `None`。
pub fn attribute_label(attribute: &str) -> Option<&'static str> {
    ATTRIBUTES.iter().find(|a| a.value == attribute).map(|a| a.label)
}

/// サブ属性の表示名 (`pretty` → 「プリティ」)。表に無い値は `None`。
pub fn sub_attribute_label(sub_attribute: &str) -> Option<&'static str> {
    SUB_ATTRIBUTES.iter().find(|t| t.value == sub_attribute).map(|t| t.label)
}

/// サブ属性の属性 (`pretty` → `princess`)。
pub fn sub_attribute_attribute(sub_attribute: &str) -> Option<&'static str> {
    SUB_ATTRIBUTES.iter().find(|t| t.value == sub_attribute).map(|t| t.attribute)
}

/// サブ属性の正式な呼び名 (`pretty` → 「プリンセス プリティ」)。公式の読みどおり属性を前に付ける。
pub fn sub_attribute_full_name(sub_attribute: &str) -> Option<String> {
    let t = SUB_ATTRIBUTES.iter().find(|t| t.value == sub_attribute)?;
    Some(format!("{} {}", attribute_label(t.attribute)?, t.label))
}

/// プロフィールに出す属性の文 (「プリンセス」/「プリンセス プリティ」)。
/// 表に無い属性 (学マスの「1年」のような学年) は出さない。
pub fn attribute_display(attribute: Option<&str>, sub_attribute: Option<&str>) -> Option<String> {
    let attr = attribute_label(attribute.filter(|a| !a.is_empty())?)?;
    match sub_attribute.filter(|t| !t.is_empty()).and_then(sub_attribute_label) {
        // 属性と食い違うサブ属性は付けない (データの誤りを画面で増幅しない)。
        Some(label) if sub_attribute.and_then(sub_attribute_attribute) == attribute => Some(format!("{attr} {label}")),
        _ => Some(attr.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn million_and_765_share_three_attributes() {
        let ml: Vec<String> = attribute_options("ml").into_iter().map(|o| o.label).collect();
        assert_eq!(ml, vec!["プリンセス", "フェアリー", "エンジェル"]);
        assert_eq!(attribute_options("765as"), attribute_options("ml"));
        assert!(attribute_options("gakuen").is_empty());
    }

    #[test]
    fn each_attribute_splits_into_two_sub_attributes() {
        for attr in ["princess", "fairy", "angel"] {
            assert_eq!(sub_attribute_options(attr).len(), 2, "{attr}");
        }
        assert!(sub_attribute_options("cute").is_empty());
        let princess: Vec<String> = sub_attribute_options("princess").into_iter().map(|o| o.value).collect();
        assert_eq!(princess, vec!["pretty", "girly"]);
    }

    #[test]
    fn every_sub_attribute_belongs_to_a_known_attribute() {
        for t in SUB_ATTRIBUTES {
            assert!(attribute_label(t.attribute).is_some(), "{}", t.value);
        }
    }

    /// `tools/apply_data.py` の検査 (`IDOL_SUB_ATTRIBUTES`) がこの表と同じ組を持つ。
    #[test]
    fn apply_data_knows_the_same_sub_attributes() {
        let py = include_str!("../../../tools/apply_data.py");
        for t in SUB_ATTRIBUTES {
            assert!(py.contains(&format!("\"{}\": \"{}\"", t.value, t.attribute)), "{}", t.value);
        }
    }

    #[test]
    fn display_joins_attribute_and_sub_attribute() {
        assert_eq!(attribute_display(Some("princess"), Some("pretty")).as_deref(), Some("プリンセス プリティ"));
        assert_eq!(attribute_display(Some("princess"), None).as_deref(), Some("プリンセス"));
        assert_eq!(attribute_display(Some("cute"), Some("")).as_deref(), Some("キュート"));
        // 属性と合わないサブ属性は付けない。
        assert_eq!(attribute_display(Some("fairy"), Some("pretty")).as_deref(), Some("フェアリー"));
        assert_eq!(attribute_display(Some("1年"), None), None);
        assert_eq!(attribute_display(None, Some("pretty")), None);
        assert_eq!(sub_attribute_full_name("breaker").as_deref(), Some("フェアリー ブレイカー"));
    }
}
