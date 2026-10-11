//! 呼び方 (誰が誰をどう呼ぶか) の画面の射影。データは [`crate::domain::personas`]。
//!
//! - アイドル詳細の「呼び方」: 相手 1 人につき 1 行で「本人 → 相手」と「相手 → 本人」を並べる。
//!   ユニットの仲間を先頭にし、ブランドごとにまとめる。先頭の数行 (区画) と全員 (シート) を同じ並びで返す。
//! - ユニット詳細の「呼称表」: メンバー × (メンバー + プロデューサー) のマス目。対角線は一人称。
//!
//! 並べ方・まとめ方・検索の照合・コピーの文面はここで決め、画面は描くだけにする。

use std::collections::{BTreeMap, HashSet};

use crate::domain::personas::{self, Form};
use crate::domain::snapshot::Snapshot;
use crate::domain::text_search_index::FoldedNeedle;

/// 「ユニットの仲間」とみなすユニットの人数の上限。これより大きい編成 (全体曲の名義・ブランド全員) は
/// 仲間としてまとめない (ブランドの全員が仲間になってしまう)。
pub const UNIT_MATE_MAX_MEMBERS: usize = 12;

/// 呼称表を出すユニットの人数の上限。これより大きい編成 (ブランド全員の名義など) はマス目が数万になり、
/// 表として読めないので出さない (横に流して読めるのはこのくらいまで)。
pub const UNIT_CALL_MATRIX_MAX_MEMBERS: usize = 16;

/// アイドル詳細の区画に出す行数。
pub const CALL_PREVIEW_LIMIT: usize = 3;

/// どちら向きの呼び方を出すか。
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallDirection {
    /// どちらか一方でも登録がある相手。
    Both,
    /// 本人が相手を呼ぶ登録がある相手だけ。
    Outgoing,
    /// 相手が本人を呼ぶ登録がある相手だけ。
    Incoming,
}

/// 相手 1 人ぶんの呼び方。
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct IdolCallPair {
    pub other_id: String,
    pub other_name: String,
    /// 本人 → 相手。未登録は `None` (画面は「—」)。
    pub outgoing: Option<String>,
    /// 相手 → 本人。未登録は `None`。
    pub incoming: Option<String>,
    /// 長押しのコピーで写す文面 (「三峰結華→田中摩美々「まみみん」」を登録のある向きだけ改行で)。
    pub copy_text: String,
}

/// 見出しでまとめた行。
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct IdolCallGroup {
    /// 見出し (ユニット名・「ほかの シャニマス」・ブランドの略称)。
    pub title: String,
    pub pairs: Vec<IdolCallPair>,
}

/// アイドル詳細の「呼び方」区画。
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq, Default)]
pub struct IdolCallSummary {
    /// 呼び方のある相手の人数 (どちらかの向きに登録がある人)。0 なら区画ごと出さない。
    pub total: u32,
    /// 先頭の数行 (シートと同じ並びの頭から)。
    pub preview: Vec<IdolCallPair>,
}

/// 本人と相手の組をまとめた並び (検索・向きの絞り込み前)。
fn grouped_pairs(snap: &Snapshot, idol_id: &str) -> Vec<IdolCallGroup> {
    let Some(&self_ii) = snap.idol_index_by_id.get(idol_id) else { return Vec::new() };
    let me = &snap.idols[self_ii as usize];

    // 相手 id → (本人→相手, 相手→本人)。マスタに居ない相手は落とす。
    let mut by_other: BTreeMap<String, (Option<String>, Option<String>)> = BTreeMap::new();
    for (to, called) in personas::call_names_from(idol_id) {
        by_other.entry(to).or_default().0 = Some(called);
    }
    for (from, called) in personas::call_names_to(idol_id) {
        if from != personas::PRODUCER {
            by_other.entry(from).or_default().1 = Some(called);
        }
    }
    by_other.retain(|id, _| snap.idol_index_by_id.contains_key(id));
    if by_other.is_empty() {
        return Vec::new();
    }
    let pair_of = |other_ii: u32| -> Option<IdolCallPair> {
        let other = &snap.idols[other_ii as usize];
        let (outgoing, incoming) = by_other.get(&other.id)?.clone();
        let copy_text = [
            outgoing.as_ref().map(|c| format!("{}→{}「{c}」", me.name, other.name)),
            incoming.as_ref().map(|c| format!("{}→{}「{c}」", other.name, me.name)),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");
        Some(IdolCallPair { other_id: other.id.clone(), other_name: other.name.clone(), outgoing, incoming, copy_text })
    };

    let mut placed: HashSet<u32> = HashSet::new();
    placed.insert(self_ii);
    let mut groups = Vec::new();

    // 1) ユニットの仲間。少人数のユニットから (結びつきが強い順)、同じ人数なら名前順。
    let mut units: Vec<u32> = snap.units_by_idol.get(self_ii as usize).cloned().unwrap_or_default();
    units.retain(|&ui| snap.members_by_unit[ui as usize].len() <= UNIT_MATE_MAX_MEMBERS);
    units.sort_by(|&a, &b| {
        snap.members_by_unit[a as usize]
            .len()
            .cmp(&snap.members_by_unit[b as usize].len())
            .then_with(|| snap.units[a as usize].name.cmp(&snap.units[b as usize].name))
    });
    for ui in units {
        let mut pairs = Vec::new();
        for &ii in &snap.members_by_unit[ui as usize] {
            if placed.contains(&ii) {
                continue;
            }
            if let Some(pair) = pair_of(ii) {
                placed.insert(ii);
                pairs.push(pair);
            }
        }
        if !pairs.is_empty() {
            groups.push(IdolCallGroup { title: snap.units[ui as usize].name.clone(), pairs });
        }
    }

    // 2) ブランドごと。本人のブランドを先頭 (「ほかの 〇〇」)、残りはブランドの並び。ブランドの無い相手は最後。
    let brand_of = |ii: u32| snap.idols[ii as usize].brand_id.clone();
    let mut brand_ids: Vec<Option<String>> = Vec::new();
    for &ii in &snap.idol_order {
        if placed.contains(&ii) || !by_other.contains_key(&snap.idols[ii as usize].id) {
            continue;
        }
        let b = brand_of(ii);
        if !brand_ids.contains(&b) {
            brand_ids.push(b);
        }
    }
    let brand_rank = |b: &Option<String>| -> (u8, i64) {
        match b {
            Some(id) if Some(id) == me.brand_id.as_ref() => (0, 0),
            Some(id) => (1, snap.brand(id).map(|br| br.sort_order).unwrap_or(i64::MAX)),
            None => (2, 0),
        }
    };
    brand_ids.sort_by_key(|b| brand_rank(b));
    for b in brand_ids {
        let pairs: Vec<IdolCallPair> = snap
            .idol_order
            .iter()
            .copied()
            .filter(|ii| !placed.contains(ii) && brand_of(*ii) == b)
            .filter_map(pair_of)
            .collect();
        if pairs.is_empty() {
            continue;
        }
        let short = b.as_deref().and_then(|id| snap.brand(id)).map(|br| br.short_name.clone());
        let title = match (&b, short) {
            (Some(id), Some(short)) if Some(id) == me.brand_id.as_ref() => format!("ほかの {short}"),
            (_, Some(short)) => short,
            _ => "そのほか".to_string(),
        };
        groups.push(IdolCallGroup { title, pairs });
    }
    groups
}

/// アイドル詳細の区画: 人数と先頭の [`CALL_PREVIEW_LIMIT`] 行。
pub fn idol_call_summary(snap: &Snapshot, idol_id: &str) -> IdolCallSummary {
    let groups = grouped_pairs(snap, idol_id);
    let all: Vec<IdolCallPair> = groups.into_iter().flat_map(|g| g.pairs).collect();
    IdolCallSummary { total: all.len() as u32, preview: all.into_iter().take(CALL_PREVIEW_LIMIT).collect() }
}

/// 全員のシート: 向きと検索語 (相手の名前・読み・呼び方) で絞った、見出しごとの並び。空の見出しは出さない。
pub fn idol_call_groups(snap: &Snapshot, idol_id: &str, query: &str, direction: CallDirection) -> Vec<IdolCallGroup> {
    let needle = FoldedNeedle::new(query.trim());
    grouped_pairs(snap, idol_id)
        .into_iter()
        .filter_map(|mut g| {
            g.pairs.retain(|p| {
                let wanted = match direction {
                    CallDirection::Both => true,
                    CallDirection::Outgoing => p.outgoing.is_some(),
                    CallDirection::Incoming => p.incoming.is_some(),
                };
                wanted && (needle.is_empty() || {
                    let kana = snap.idol(&p.other_id).and_then(|i| i.name_kana.as_deref());
                    needle.matches(&p.other_name)
                        || needle.matches_opt(kana)
                        || needle.matches_opt(p.outgoing.as_deref())
                        || needle.matches_opt(p.incoming.as_deref())
                })
            });
            (!g.pairs.is_empty()).then_some(g)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// ユニットの呼称表
// ---------------------------------------------------------------------------

/// マスの種類。
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitCallCellKind {
    /// 対角線 (本人の一人称)。
    FirstPerson,
    /// メンバーどうし。
    Member,
    /// プロデューサーの列。
    Producer,
}

/// マス 1 つ。
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct UnitCallCell {
    pub kind: UnitCallCellKind,
    /// 呼び方 (一人称)。未登録は `None`。場面で分かれる一人称は「三峰 / 私」のように並べる (場面は書かない)。
    pub text: Option<String>,
}

/// ユニットの呼称表。`rows[i][j]` は `members[i]` が「`members[j]`、`j == members.len()` ならプロデューサー」を呼ぶ呼び方。
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct UnitCallMatrix {
    /// 行と列のメンバーの id (ユニットのメンバーの並び)。
    pub member_ids: Vec<String>,
    pub rows: Vec<Vec<UnitCallCell>>,
}

/// ユニットの呼称表。メンバーが 2 人未満か [`UNIT_CALL_MATRIX_MAX_MEMBERS`] を超えるか、
/// メンバーどうしの呼び方が 1 つも無いユニットは `None` (タブを出さない)。
pub fn unit_call_matrix(snap: &Snapshot, unit_id: &str) -> Option<UnitCallMatrix> {
    let &ui = snap.unit_index_by_id.get(unit_id)?;
    let members: Vec<&str> = snap.members_by_unit[ui as usize].iter().map(|&ii| snap.idols[ii as usize].id.as_str()).collect();
    if members.len() < 2 || members.len() > UNIT_CALL_MATRIX_MAX_MEMBERS {
        return None;
    }
    let short = |forms: Vec<Form>| -> Option<String> {
        let text = forms.into_iter().map(|f| f.form).collect::<Vec<_>>().join(" / ");
        (!text.is_empty()).then_some(text)
    };
    let mut any_member_call = false;
    let rows: Vec<Vec<UnitCallCell>> = members
        .iter()
        .map(|&from| {
            let mut row: Vec<UnitCallCell> = members
                .iter()
                .map(|&to| {
                    if from == to {
                        UnitCallCell { kind: UnitCallCellKind::FirstPerson, text: short(personas::first_person_forms(from)) }
                    } else {
                        let text = personas::call_name(from, to);
                        any_member_call |= text.is_some();
                        UnitCallCell { kind: UnitCallCellKind::Member, text }
                    }
                })
                .collect();
            row.push(UnitCallCell { kind: UnitCallCellKind::Producer, text: short(personas::producer_call_forms(from)) });
            row
        })
        .collect();
    any_member_call.then(|| UnitCallMatrix { member_ids: members.iter().map(|m| m.to_string()).collect(), rows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::bundle_snapshot;

    const YUIKA: &str = "sc_三峰結華";

    #[test]
    fn ユニットの仲間が先頭に来て見出しはユニット名() {
        let snap = bundle_snapshot();
        let groups = grouped_pairs(snap, YUIKA);
        assert_eq!(groups[0].title, "アンティーカ", "{:?}", groups.iter().map(|g| &g.title).collect::<Vec<_>>());
        let mamimi = groups[0].pairs.iter().find(|p| p.other_id == "sc_田中摩美々").expect("摩美々がいる");
        assert_eq!(mamimi.outgoing.as_deref(), Some("まみみん"));
        assert!(mamimi.copy_text.starts_with("三峰結華→田中摩美々「まみみん」"), "{}", mamimi.copy_text);
        // 同じ相手は 1 度だけ、本人は出ない。
        let ids: Vec<&String> = groups.iter().flat_map(|g| g.pairs.iter().map(|p| &p.other_id)).collect();
        let unique: HashSet<&&String> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len());
        assert!(!ids.iter().any(|id| *id == YUIKA));
        // 本人のブランドの残りは「ほかの」。
        assert!(groups.iter().any(|g| g.title == "ほかの シャニマス"), "{:?}", groups.iter().map(|g| &g.title).collect::<Vec<_>>());
    }

    #[test]
    fn 区画は人数と先頭3行() {
        let snap = bundle_snapshot();
        let s = idol_call_summary(snap, YUIKA);
        assert!(s.total >= 10, "{}", s.total);
        assert_eq!(s.preview.len(), CALL_PREVIEW_LIMIT);
        assert_eq!(s.preview, grouped_pairs(snap, YUIKA)[0].pairs.iter().take(3).cloned().collect::<Vec<_>>());
        assert_eq!(idol_call_summary(snap, "no-such-idol"), IdolCallSummary::default());
    }

    #[test]
    fn 向きと検索で絞る() {
        let snap = bundle_snapshot();
        let out = idol_call_groups(snap, YUIKA, "", CallDirection::Outgoing);
        assert!(out.iter().flat_map(|g| &g.pairs).all(|p| p.outgoing.is_some()));
        let inc = idol_call_groups(snap, YUIKA, "", CallDirection::Incoming);
        assert!(inc.iter().flat_map(|g| &g.pairs).all(|p| p.incoming.is_some()));
        let hit = idol_call_groups(snap, YUIKA, "まみみん", CallDirection::Both);
        let ids: Vec<&String> = hit.iter().flat_map(|g| g.pairs.iter().map(|p| &p.other_id)).collect();
        assert_eq!(ids, ["sc_田中摩美々"]);
        // 読みでも当たる。
        let by_kana = idol_call_groups(snap, YUIKA, "たなかまみみ", CallDirection::Both);
        assert_eq!(by_kana.iter().map(|g| g.pairs.len()).sum::<usize>(), 1);
        assert!(idol_call_groups(snap, YUIKA, "存在しない呼び方", CallDirection::Both).is_empty());
    }

    #[test]
    fn 呼称表は対角線が一人称で最後の列がプロデューサー() {
        let snap = bundle_snapshot();
        let antica = snap.units.iter().find(|u| u.name == "アンティーカ").unwrap();
        let m = unit_call_matrix(snap, &antica.id).expect("アンティーカの呼称表");
        let n = m.member_ids.len();
        assert_eq!(n, 5);
        for (i, row) in m.rows.iter().enumerate() {
            assert_eq!(row.len(), n + 1);
            assert_eq!(row[i].kind, UnitCallCellKind::FirstPerson);
            assert_eq!(row[n].kind, UnitCallCellKind::Producer);
        }
        let yi = m.member_ids.iter().position(|id| id == YUIKA).unwrap();
        assert_eq!(m.rows[yi][yi].text.as_deref(), Some("三峰 / 私"));
        assert_eq!(m.rows[yi][n].text.as_deref(), Some("Pたん"));
        assert_eq!(unit_call_matrix(snap, "no-such-unit"), None);
        // ブランド全員の名義 (193 人) は表にしない。
        let all = snap.units.iter().find(|u| u.name == "THE IDOLM@STER CINDERELLA GIRLS!!").unwrap();
        assert_eq!(unit_call_matrix(snap, &all.id), None);
    }
}
