import Foundation
import Observation

/// SongTagPicker のデータ取得・付与担当 (Presentation)。
///
/// タグ候補 (全件を 1 回取り、検索は手元で絞る)・付与済み判定・付与実行・付与完了シェアの組み立てを持つ。
/// 曲/ブランドの解決は `SongReading` / `BrandReading` ポート越し (AppDatabase 直叩きを排除)。
@MainActor
@Observable
final class SongTagPickerViewModel {
    let songId: String

    /// 候補の全件 (人気順)。検索欄の絞り込みは `pool.matching` で手元で行う。
    private(set) var pool = TagSearchPool()
    private(set) var myTagIds: Set<String> = []
    private(set) var isLoading = false
    private(set) var isApplying = false
    var applyError: String?

    private let tagReading: any CommunityTagReading
    private let tagWriting: any CommunityTagWriting
    private let songReading: any SongReading
    private let brandReading: any BrandReading
    /// View の init (nonisolated) から生成できるよう init も nonisolated にする。
    nonisolated init(
        songId: String,
        tagReading: any CommunityTagReading = AppContainer.shared.communityTagReading,
        tagWriting: any CommunityTagWriting = AppContainer.shared.communityTagWriting,
        songReading: any SongReading = AppContainer.shared.songReading,
        brandReading: any BrandReading = AppContainer.shared.brandReading
    ) {
        self.songId = songId
        self.tagReading = tagReading
        self.tagWriting = tagWriting
        self.songReading = songReading
        self.brandReading = brandReading
    }

    func loadData() async {
        isLoading = true
        defer { isLoading = false }
        async let tagResult = tagReading.tags(search: "", category: "", sort: "popular")
        async let songTagResult = tagReading.songTags(songId: songId)
        pool.replace((try? await tagResult) ?? [])
        if let result = try? await songTagResult {
            myTagIds = Set(result.myTagIds)
        }
    }

    /// 検索していないときの見出し分け (ムード・シーン …)。まとめ方はコア。中はよく使われる順。
    var categoryGroups: [TagCategorySection] {
        pool.all.groupedByCategory { songTagCategoryGroups(tags: $0) }
    }

    /// 新規タグ作成直後に候補一覧の先頭へ即時反映する。
    func insertCreated(_ tag: CommunityTag) {
        pool.insert(tag)
    }

    /// 選択タグをまとめて付与する。成功時は完了シェア用のコンテキストを返す。
    func applyTags(_ selectedTagIds: Set<String>, selectedTagsById: [String: CommunityTag], song: SongWithArtists?) async -> TagShareContext? {
        guard !selectedTagIds.isEmpty else { return nil }
        isApplying = true
        defer { isApplying = false }
        do {
            try await tagWriting.applySongTags(songId: songId, tagIds: Array(selectedTagIds))
            let appliedTags = selectedTagIds.compactMap { selectedTagsById[$0] }
            return await makeShareContext(applied: appliedTags, song: song)
        } catch {
            applyError = (error as? APIClientError)?.errorDescription ?? error.localizedDescription
            return nil
        }
    }

    /// 付与完了シェアの内容を組み立てる。seed は曲のブランドカラー → 先頭タグ色。
    private func makeShareContext(applied: [CommunityTag], song: SongWithArtists?) async -> TagShareContext {
        let resolvedSong: Song?
        if let loaded = song?.song {
            resolvedSong = loaded
        } else {
            resolvedSong = try? await songReading.song(id: songId)
        }
        var brandColor: String?
        if let bid = resolvedSong?.brandId {
            brandColor = (try? await brandReading.brands())?.first { $0.id == bid }?.color
        }
        return TagShareContext(
            songTitle: resolvedSong?.title ?? "この曲",
            artistNames: song?.artistNames ?? resolvedSong?.singerLabel,
            tags: applied,
            seed: ColorMath.firstValidHex(brandColor, applied.first?.color?.rawValue),
            artworkUrl: resolvedSong?.artworkUrl
        )
    }
}
