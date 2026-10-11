import Foundation
import GRDB

/// アイドルの項目 1 値 (`idol_facets`。性格・好み・目の色・眼鏡…)。**マスタ**なので同期で降ってくる。
///
/// 項目の定義 (語彙・型) も、画面に出す形 (見た目の版・項目の並び) も共有コア
/// (`imas-core` `domain/idol_facets.rs` / `domain/idol_appearance.rs`) が決める。
/// この型は GRDB へ保存するだけの器で、画面は `SnapshotStore.idolProfileExtras` を読む。
struct IdolFacetRecord: Codable, FetchableRecord, PersistableRecord, Identifiable, Hashable, Sendable {
    static let databaseTableName = "idol_facets"

    var id: String
    var idolId: String
    var facet: String
    var value: String
    /// `official` / `promoted`。
    var origin: String
    var sourceNote: String?
    var sortOrder: Int64

    enum CodingKeys: String, CodingKey {
        case id, facet, value, origin
        case idolId = "idol_id"
        case sourceNote = "source_note"
        case sortOrder = "sort_order"
    }
}

/// アイドルの髪型 1 つぶん (`idol_hairstyles`)。1 人に複数 (基本 / セカンドヘア …)、`is_main` はちょうど 1 つ。
///
/// `styles` / `accessories` は複数値で、`、` 区切りの 1 文字列のまま持つ (割るのは共有コア)。
struct IdolHairstyleRecord: Codable, FetchableRecord, PersistableRecord, Identifiable, Hashable, Sendable {
    static let databaseTableName = "idol_hairstyles"

    var id: String
    var idolId: String
    var label: String
    var isMain: Bool
    var hairColor: String?
    var hairColorSecondary: String?
    var hairLength: String?
    var styles: String?
    var bangs: String?
    var accessories: String?
    var origin: String
    var sourceNote: String?
    var sortOrder: Int64

    enum CodingKeys: String, CodingKey {
        case id, label, styles, bangs, accessories, origin
        case idolId = "idol_id"
        case isMain = "is_main"
        case hairColor = "hair_color"
        case hairColorSecondary = "hair_color_secondary"
        case hairLength = "hair_length"
        case sourceNote = "source_note"
        case sortOrder = "sort_order"
    }
}
