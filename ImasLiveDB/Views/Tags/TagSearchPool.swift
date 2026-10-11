import Foundation

/// タグを選ぶ画面の候補の置き場。全件を 1 回だけ取り、検索欄の絞り込みは手元で行う。
///
/// 以前は打鍵ごとにサーバへ `search=` を投げていたが、古い語の応答が後から届くと
/// 新しい語 (消して空にした場合も) の結果を上書きし、「文字を消しても検索し直されない」
/// 状態になっていた。全件は最初に取っているので、手元で絞れば応答の順序の問題が起きない
/// (打鍵ごとに D1 を読まない利点もある)。照合はコア (`domain/text_search_index.rs`) に一任する。
struct TagSearchPool {
    /// 全件 (取ってきた並び = 人気順)。
    private(set) var all: [CommunityTag]
    private var catalog: TextSearchCatalog

    init(_ tags: [CommunityTag] = []) {
        all = tags
        catalog = Self.makeCatalog(tags)
    }

    mutating func replace(_ tags: [CommunityTag]) {
        self = TagSearchPool(tags)
    }

    /// 作成直後のタグを先頭に足す (同じ id が既にあれば何もしない)。
    mutating func insert(_ tag: CommunityTag) {
        guard !all.contains(where: { $0.id == tag.id }) else { return }
        replace([tag] + all)
    }

    /// 検索語に当たるタグ。空なら全件。名前と説明を見る。
    func matching(_ needle: String) -> [CommunityTag] {
        let trimmed = needle.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return all }
        return catalog.filter(all, needle: trimmed)
    }

    /// 同じ名前のタグが既にあるか (「〜を作成」を出すかどうか)。絞り込み前の全件で見る。
    func containsName(_ name: String) -> Bool {
        all.contains { $0.name == name }
    }

    private static func makeCatalog(_ tags: [CommunityTag]) -> TextSearchCatalog {
        TextSearchCatalog(fieldsPerItem: tags.map { [$0.name, $0.description] })
    }
}

/// タグの見出し分けの 1 まとまり (見出しと、その中のタグ)。
struct TagCategorySection: Identifiable {
    /// カテゴリの生値。カテゴリの無いタグのまとまりは空文字。
    let category: String
    let label: String
    let tags: [CommunityTag]
    var id: String { category }
}

extension Array where Element == CommunityTag {
    /// カテゴリごとにまとめる。まとめ方 (並び・「その他」) はコアの `grouper` が決め、
    /// 各まとまりの中は今の並びのまま。
    func groupedByCategory(_ grouper: ([TagCategoryInput]) -> [TagCategoryGroup]) -> [TagCategorySection] {
        let byId = Dictionary(map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        return grouper(map { TagCategoryInput(id: $0.id, category: $0.category?.rawValue) }).map { group in
            TagCategorySection(category: group.category, label: group.label, tags: group.tagIds.compactMap { byId[$0] })
        }
    }
}

/// タグの一覧の並べ方。1 本の一覧 (今の並び順のまま) か、カテゴリごとの見出しに分けるか。
enum TagListLayout: CaseIterable, Hashable {
    case flat
    case byCategory

    var label: String {
        switch self {
        case .flat: "一覧"
        case .byCategory: "カテゴリ別"
        }
    }
}
