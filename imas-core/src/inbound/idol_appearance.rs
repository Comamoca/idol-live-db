//! アイドル詳細のプロフィール (見た目・話し方・呼び方) と、ユニットの呼称表の FFI 面。
//! 規則は [`crate::domain::idol_appearance`] と [`crate::domain::call_sheet`]。

use super::snapshot_store::{SnapshotError, SnapshotStore};
use crate::domain::call_sheet::{self, CallDirection, IdolCallGroup, UnitCallMatrix};
use crate::domain::idol_appearance::{self as domain, IdolProfileExtras};
use crate::domain::idol_queries::IdolRecord;

#[uniffi::export]
impl SnapshotStore {
    /// プロフィールのタブの見た目・話し方・呼び方 (区画の先頭の数行と人数)。
    pub fn idol_profile_extras(&self, idol_id: String) -> Result<IdolProfileExtras, SnapshotError> {
        let snap = self.current()?;
        Ok(domain::idol_profile_extras(&snap, &idol_id))
    }

    /// 呼び方の全員 (シート)。`query` は相手の名前・読み・呼び方、`direction` は向き。
    pub fn idol_call_groups(
        &self,
        idol_id: String,
        query: String,
        direction: CallDirection,
    ) -> Result<Vec<IdolCallGroup>, SnapshotError> {
        let snap = self.current()?;
        Ok(call_sheet::idol_call_groups(&snap, &idol_id, &query, direction))
    }

    /// 同じ髪型 (`style`) の版を持つアイドル (公式順)。
    pub fn idols_by_hair_style(&self, style: String) -> Result<Vec<IdolRecord>, SnapshotError> {
        let snap = self.current()?;
        Ok(domain::idol_indices_with_hair_style(&snap, &style)
            .into_iter()
            .map(|ii| IdolRecord::from(&snap.idols[ii as usize]))
            .collect())
    }

    /// ユニットの呼称表。メンバーどうしの呼び方が 1 つも無いユニットは nil (タブを出さない)。
    pub fn unit_call_matrix(&self, unit_id: String) -> Result<Option<UnitCallMatrix>, SnapshotError> {
        let snap = self.current()?;
        Ok(call_sheet::unit_call_matrix(&snap, &unit_id))
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::call_sheet::CallDirection;
    use crate::test_support::bundle_store;

    /// FFI 面の疎通 (中身の規則は domain 側のテスト)。
    #[test]
    fn ffi_surface_smoke() {
        let store = bundle_store();
        let extras = store.idol_profile_extras("sc_三峰結華".into()).unwrap();
        assert!(extras.speech.is_some());
        assert!(extras.calls.total > 0);
        assert!(!store.idol_call_groups("sc_三峰結華".into(), String::new(), CallDirection::Both).unwrap().is_empty());
        assert!(!store.idols_by_hair_style("ツインテール".into()).unwrap().is_empty());
        let unit = store.unit_index_record().unwrap().units.into_iter().find(|u| u.name == "アンティーカ").unwrap();
        assert!(store.unit_call_matrix(unit.id).unwrap().is_some());
    }
}
