import GRDB
import XCTest
@testable import ImasLiveDB

/// 髪型の取り込み。端末の表には「main は 1 人 1 つ」「label は 1 人の中で重ならない」の一意索引が
/// (同梱 DB と起動時のコアのスキーマ適用で) あるので、付け替え・label の変更が削除や別の束より先に
/// 届いても、新しい行が捨てられないこと。
final class IdolHairstyleSyncTests: XCTestCase {

    private func makeDatabase() throws -> AppDatabase {
        let queue = try makeMigratedDatabase()
        try queue.write { db in
            try db.execute(sql: """
                CREATE UNIQUE INDEX IF NOT EXISTS idx_idol_hairstyles_main ON idol_hairstyles(idol_id) WHERE is_main = 1;
                CREATE UNIQUE INDEX IF NOT EXISTS idx_idol_hairstyles_label ON idol_hairstyles(idol_id, label);
                """)
        }
        return try AppDatabase(dbQueue: queue)
    }

    private func row(_ id: String, label: String, main: Bool) -> IdolHairstyleRecord {
        IdolHairstyleRecord(id: id, idolId: "i1", label: label, isMain: main, hairColor: "黒", hairColorSecondary: nil,
                            hairLength: nil, styles: nil, bangs: nil, accessories: nil, origin: "official",
                            sourceNote: nil, sortOrder: 0)
    }

    private func rows(_ db: AppDatabase) throws -> [IdolHairstyleRecord] {
        try db.dbQueue.read { try IdolHairstyleRecord.order(Column("id")).fetchAll($0) }
    }

    /// main の付け替え (A → B) で、B だけが先に届いても B は入り、A は main でなくなる。
    func testNewMainArrivingFirstTakesOverTheMain() throws {
        let db = try makeDatabase()
        try db.upsertIdolHairstyles([row("a", label: "基本", main: true), row("b", label: "セカンドヘア", main: false)])
        try db.upsertIdolHairstyles([row("b", label: "セカンドヘア", main: true)])
        let got = try rows(db)
        XCTAssertEqual(got.map(\.id), ["a", "b"])
        XCTAssertEqual(got.map(\.isMain), [false, true])
    }

    /// label を変えた行 (id が変わる) が、旧行の削除より先に届いても入る。
    func testRelabeledRowReplacesTheOldRowWithTheSameLabel() throws {
        let db = try makeDatabase()
        try db.upsertIdolHairstyles([row("old", label: "基本", main: true)])
        try db.upsertIdolHairstyles([row("new", label: "基本", main: true)])
        XCTAssertEqual(try rows(db).map(\.id), ["new"])
    }
}
