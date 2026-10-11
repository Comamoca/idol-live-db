//! 写す側 (サーバーなど、端末でない利用者) の運用の規則。
//!
//! アプリの同期 ([`crate::domain::sync_planning`] / [`crate::domain::sync_decisions`]) は端末の DB が前提で、
//! サーバーで定期的に走らせるときに足りない判断がある。それをここに集める。**アプリの挙動は変えない**
//! (アプリの関数は触らず、足りない分を別の関数にしてある)。
//!
//! | 規則 | アプリ | 写す側 (ここ) |
//! |---|---|---|
//! | 差分の起点 | 前回の実行を始めた時刻ちょうど | **2 時間前まで遡る** ([`incremental_since`]) |
//! | 孤児の掃除 | 単一主キーの表だけ。`valid_ids` が空でなければ全部消す | **複合主キーの表も**、**1 回の掃除で型の行数の 5% まで** ([`should_delete_orphans`] [`orphan_delete_allowed`]) |
//! | 型の埋め直し | 端末の表が空のときだけ (アプリ固有) | **一度も取り切っていない型は epoch から** ([`step_start`]) |
//! | 取得の失敗 | iOS の `CKError` だけ再試行を知る | **HTTP の失敗も分類**、続きのページの NOT_FOUND は「型が無い」でなくマーカーの失効 ([`classify_fetch_error`]) |
//!
//! 時刻の単位は他の同期関数と同じ**秒 (f64)**。

use crate::domain::mirror_schema::mirror_table_info;
use crate::domain::sync_decisions::{classify_error, SyncErrorKind, SyncErrorSignal};
use crate::domain::sync_planning::{step_start_plan, SyncStepStart};

/// 差分の起点を、前回の実行を始めた時刻よりこれだけ前に取る (2 時間)。
///
/// CloudKit への書き込み (ImasLiveDB の送り込み) が同期の実行と重なると、`modifiedAt` が実行を始めた時刻より
/// **前**のレコードが、あとから見えることがある。起点が始めた時刻ぴったりだとそれが次のフルまで落ちる。
/// 重なって読み直した行は、内容が同じなら書かない (書き込みの側で `IS NOT` で比べる) ので、遡りの費用は読み取りだけ。
pub const INCREMENTAL_OVERLAP_SECONDS: f64 = 2.0 * 3600.0;

/// 差分の実行の起点。`startup_plan` が返した `modified_since_epoch` を遡らせる (`None` = フルはそのまま `None`)。
pub fn incremental_since(modified_since_epoch: Option<f64>) -> Option<f64> {
    modified_since_epoch.map(|t| (t - INCREMENTAL_OVERLAP_SECONDS).max(0.0))
}

/// 1 回の孤児掃除で消してよい割合の上限 (型の行数に対して)。
pub const ORPHAN_MAX_RATIO: f64 = 0.05;
/// 小さい型 (ブランド 8 件など) で 1 件消えただけで止まらないよう、この件数までは割合に関わらず消す。
pub const ORPHAN_MAX_FLOOR: usize = 3;

/// 孤児を消してよいか。超えるときは取りこぼし・壊れた応答を疑って**消さずに知らせる**。
///
/// アプリの [`crate::domain::sync_planning::orphan_ids`] は「`valid_ids` が空なら何も消さない」だけで、
/// 一部しか取れなかった応答では写しの大半を孤児と見なして消してしまう。端末の DB はあとで同期し直せるが、
/// サーバーの写しは次の読み出しがそのまま壊れた内容を見るので、歯止めを足した。
pub fn orphan_delete_allowed(orphans: usize, total: usize) -> bool {
    orphans <= ORPHAN_MAX_FLOOR || (orphans as f64) <= (total as f64) * ORPHAN_MAX_RATIO
}

/// このステップの完走で孤児掃除をしてよいか。全件を取り直した完走 (`started_from_epoch`) の、写す型だけ。
///
/// アプリの [`crate::domain::sync_decisions::should_delete_orphans`] は単一主キーの表だけだが、**写しは複合主キーの表も掃除する**:
/// セトリの出演者・公演の出演者などの行は CloudKit で物理削除される (tombstone が来ない) ので、全件を取り直した完走でしか消せない。
/// 見えた鍵の集め方・消す SQL は [`crate::domain::mirror_schema::MirrorStep`]、歯止めは [`orphan_delete_allowed`]。
pub fn should_delete_orphans(record_type: &str, started_from_epoch: bool) -> bool {
    started_from_epoch && mirror_table_info(record_type).is_some()
}

/// ステップの取得起点。アプリの [`step_start_plan`] に**型の埋め直し**を足したもの。
///
/// `backfilled` が偽 (その型を一度も取り切っていない) なら、差分の実行でも epoch から読む。あとから足した型や、
/// 「型が無い」で飛ばした型 (本番に未昇格の `IdolFacet` など) が、型ができたあとも差分の起点より前の行を
/// 取りこぼさないようにする。埋め直しのステップは `started_from_epoch` が偽なので孤児掃除は走らない
/// (フルの実行だけが掃除する)。
pub fn step_start(
    record_type: &str,
    is_full_sync: bool,
    done_steps: &[String],
    checkpoint_epoch: Option<f64>,
    modified_since_epoch: Option<f64>,
    backfilled: bool,
) -> SyncStepStart {
    let since = if backfilled { modified_since_epoch } else { None };
    step_start_plan(record_type, is_full_sync, done_steps, checkpoint_epoch, since)
}

/// 取得の失敗の種類。
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(
    feature = "web-export",
    derive(ts_rs::TS),
    ts(export, export_to = "../../web/wasm/imas-sync-wasm/types/", rename_all = "snake_case")
)]
pub enum MirrorErrorKind {
    /// この環境にそのレコードタイプがまだ無い (production への deploy 待ち)。そのステップだけ飛ばして続ける。
    /// **取り切った扱いにはしない** (型ができた次の実行で epoch から取る)。
    UnknownRecordType,
    /// `modifiedAt` が QUERYABLE / SORTABLE でない。Dashboard の設定が要る。
    SchemaNotQueryable,
    /// 待てば直る (通信断・429・5xx)。この実行は止めて、次の実行が同じところから続ける。
    Retryable,
    /// トークンが違う・権限が無い (401 / 403)。直るまで進まない。
    Auth,
    /// それ以外。続きのページなら、マーカーが失効した疑いで張り直す。
    Fatal,
}

/// 取得の失敗を分類する。`has_marker` は、失敗したのが続きのページ (continuationMarker あり) か。
///
/// アプリの [`classify_error`] を土台に、写す側の事情を足す:
/// - **続きのページの「型が無い」はマーカーの失効**であって型の不在ではない (最初のページだけが型の不在を言える)。
///   型の不在と取り違えると、そのステップが取り切った扱いで終わり、残りが次のフルまで落ちる。
/// - HTTP の 421 / 429 / 5xx と、HTTP に届かなかった失敗 (通信断・タイムアウト) は再試行できる。
///   アプリは Android で再試行を使わない (`ck_error_code` を持たない) ので、そちらは分類に入っていない。
/// - 401 / 403 は認証。
pub fn classify_fetch_error(signal: &SyncErrorSignal, has_marker: bool) -> MirrorErrorKind {
    match classify_error(signal) {
        SyncErrorKind::UnknownRecordType | SyncErrorKind::SchemaNotQueryable if has_marker => MirrorErrorKind::Fatal,
        SyncErrorKind::UnknownRecordType => MirrorErrorKind::UnknownRecordType,
        SyncErrorKind::SchemaNotQueryable => MirrorErrorKind::SchemaNotQueryable,
        SyncErrorKind::Retryable => MirrorErrorKind::Retryable,
        SyncErrorKind::Fatal => match signal.http_status {
            Some(401) | Some(403) => MirrorErrorKind::Auth,
            Some(421) | Some(429) | Some(500..=599) => MirrorErrorKind::Retryable,
            None if signal.ck_error_code.is_none() && !signal.is_cloudkit_error => MirrorErrorKind::Retryable,
            _ => MirrorErrorKind::Fatal,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn http(status: Option<i32>, code: Option<&str>, reason: Option<&str>) -> SyncErrorSignal {
        SyncErrorSignal {
            is_cloudkit_error: status.is_some(),
            ck_error_code: None,
            http_status: status,
            server_error_code: code.map(String::from),
            reason: reason.map(String::from),
            message: "m".into(),
            retry_after_seconds: None,
        }
    }

    #[test]
    fn since_goes_back_two_hours_but_never_below_zero() {
        assert_eq!(incremental_since(None), None);
        assert_eq!(incremental_since(Some(100_000.0)), Some(100_000.0 - 7200.0));
        assert_eq!(incremental_since(Some(60.0)), Some(0.0));
    }

    #[test]
    fn orphan_guard_allows_a_few_or_five_percent() {
        assert!(orphan_delete_allowed(0, 0));
        assert!(orphan_delete_allowed(3, 8)); // 小さい型は 3 件まで
        assert!(!orphan_delete_allowed(4, 8));
        assert!(orphan_delete_allowed(60, 1200)); // ちょうど 5%
        assert!(!orphan_delete_allowed(61, 1200));
        assert!(!orphan_delete_allowed(1000, 1200)); // 壊れた応答で大半が孤児に見える
    }

    #[test]
    fn a_never_completed_type_starts_from_epoch_even_in_an_incremental_run() {
        let s = step_start("IdolFacet", false, &[], None, Some(5000.0), false);
        assert_eq!(s.start_epoch, 0.0);
        assert!(!s.started_from_epoch, "埋め直しでは孤児掃除をしない");
        let s = step_start("Idol", false, &[], None, Some(5000.0), true);
        assert_eq!(s.start_epoch, 5000.0);
        let full = step_start("Idol", true, &[], None, None, true);
        assert!(full.started_from_epoch && full.start_epoch == 0.0);
        let resumed = step_start("Idol", true, &["Idol".to_string()], None, None, true);
        assert!(resumed.skip);
    }

    #[test]
    fn orphans_are_swept_only_after_a_from_scratch_run() {
        assert!(should_delete_orphans("Song", true));
        assert!(should_delete_orphans("IdolFacet", true));
        assert!(!should_delete_orphans("Song", false));
        assert!(should_delete_orphans("SongArtist", true), "複合主キーも掃除する");
        assert!(!should_delete_orphans("Nope", true));
    }

    #[test]
    fn missing_type_is_only_believed_on_the_first_page() {
        let not_found = http(Some(404), Some("NOT_FOUND"), Some("Did not find record type: IdolFacet"));
        assert_eq!(classify_fetch_error(&not_found, false), MirrorErrorKind::UnknownRecordType);
        assert_eq!(classify_fetch_error(&not_found, true), MirrorErrorKind::Fatal);
        // 400 の "Unknown type"
        let unknown = http(Some(400), None, Some("Unknown type: IdolFacet"));
        assert_eq!(classify_fetch_error(&unknown, false), MirrorErrorKind::UnknownRecordType);
        // フィールドのスキーマ不備は型の不在ではない
        let field = http(Some(400), None, Some("Field 'modifiedAt' is not marked queryable"));
        assert_eq!(classify_fetch_error(&field, false), MirrorErrorKind::Fatal);
    }

    #[test]
    fn http_failures_are_classified() {
        assert_eq!(classify_fetch_error(&http(Some(401), None, None), false), MirrorErrorKind::Auth);
        assert_eq!(classify_fetch_error(&http(Some(403), None, None), false), MirrorErrorKind::Auth);
        assert_eq!(classify_fetch_error(&http(Some(429), None, None), false), MirrorErrorKind::Retryable);
        assert_eq!(classify_fetch_error(&http(Some(503), None, None), true), MirrorErrorKind::Retryable);
        assert_eq!(classify_fetch_error(&http(Some(421), None, None), false), MirrorErrorKind::Retryable);
        assert_eq!(classify_fetch_error(&http(Some(400), None, Some("bad")), false), MirrorErrorKind::Fatal);
        // HTTP に届かなかった (通信断・タイムアウト)
        assert_eq!(classify_fetch_error(&http(None, None, None), false), MirrorErrorKind::Retryable);
    }
}
