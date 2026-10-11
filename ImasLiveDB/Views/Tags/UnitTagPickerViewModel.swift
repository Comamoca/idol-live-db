import Foundation
import Observation

/// UnitTagPicker のデータ取得・付与担当 (Presentation)。`IdolTagPickerViewModel` と同じ役割分担だが、
/// ユニットタグは曲/アイドルとは別プール (unit_tag_master)。
@MainActor
@Observable
final class UnitTagPickerViewModel {
    let unitId: String

    /// 候補の全件 (人気順)。検索欄の絞り込みは `pool.matching` で手元で行う (`TagSearchPool`)。
    private(set) var pool = TagSearchPool()
    private(set) var myTagIds: Set<String> = []
    private(set) var isLoading = false
    private(set) var isApplying = false
    var applyError: String?

    private let tagReading: any CommunityTagReading
    private let tagWriting: any CommunityTagWriting
    /// View の init (nonisolated) から生成できるよう init も nonisolated にする。
    nonisolated init(
        unitId: String,
        tagReading: any CommunityTagReading = AppContainer.shared.communityTagReading,
        tagWriting: any CommunityTagWriting = AppContainer.shared.communityTagWriting
    ) {
        self.unitId = unitId
        self.tagReading = tagReading
        self.tagWriting = tagWriting
    }

    func loadData() async {
        isLoading = true
        defer { isLoading = false }
        async let tagResult = tagReading.unitTagCatalog(search: "", category: "", sort: "popular")
        async let unitTagResult = tagReading.unitTags(unitId: unitId)
        pool.replace((try? await tagResult) ?? [])
        if let result = try? await unitTagResult {
            myTagIds = Set(result.myTagIds)
        }
    }

    /// 新規タグ作成直後に候補一覧の先頭へ即時反映する。
    func insertCreated(_ tag: CommunityTag) {
        pool.insert(tag)
    }

    /// 選択タグをまとめて付与する。成功可否を返す。
    @discardableResult
    func applyTags(_ selectedTagIds: Set<String>) async -> Bool {
        guard !selectedTagIds.isEmpty else { return false }
        isApplying = true
        defer { isApplying = false }
        do {
            try await tagWriting.applyUnitTags(unitId: unitId, tagIds: Array(selectedTagIds))
            return true
        } catch {
            applyError = (error as? APIClientError)?.errorDescription ?? error.localizedDescription
            return false
        }
    }
}
