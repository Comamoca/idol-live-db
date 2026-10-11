import Foundation
import Observation

/// IdolTagPicker のデータ取得・付与担当 (Presentation)。`SongTagPickerViewModel` と同じ役割分担だが、
/// アイドルタグは曲とは別プール (idol_tag_master) なので付与完了シェア導線は無く、`applyTags` は成否のみ返す。
@MainActor
@Observable
final class IdolTagPickerViewModel {
    let idolId: String

    /// 候補の全件 (人気順)。検索欄の絞り込みは `pool.matching` で手元で行う (`TagSearchPool`)。
    private(set) var pool = TagSearchPool()
    /// 候補をカテゴリごとにまとめたもの (検索していないときの見出し分け)。まとめ方はコア。
    var categoryGroups: [TagCategorySection] {
        pool.all.groupedByCategory { idolTagCategoryGroups(tags: $0) }
    }
    private(set) var myTagIds: Set<String> = []
    private(set) var isLoading = false
    private(set) var isApplying = false
    var applyError: String?

    private let tagReading: any CommunityTagReading
    private let tagWriting: any CommunityTagWriting
    /// View の init (nonisolated) から生成できるよう init も nonisolated にする。
    nonisolated init(
        idolId: String,
        tagReading: any CommunityTagReading = AppContainer.shared.communityTagReading,
        tagWriting: any CommunityTagWriting = AppContainer.shared.communityTagWriting
    ) {
        self.idolId = idolId
        self.tagReading = tagReading
        self.tagWriting = tagWriting
    }

    func loadData() async {
        isLoading = true
        defer { isLoading = false }
        async let tagResult = tagReading.idolTagCatalog(search: "", category: "", sort: "popular")
        async let idolTagResult = tagReading.idolTags(idolId: idolId)
        pool.replace((try? await tagResult) ?? [])
        if let result = try? await idolTagResult {
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
            try await tagWriting.applyIdolTags(idolId: idolId, tagIds: Array(selectedTagIds))
            return true
        } catch {
            applyError = (error as? APIClientError)?.errorDescription ?? error.localizedDescription
            return false
        }
    }
}
