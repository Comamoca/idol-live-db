import SwiftUI

/// 自分の投稿累計の内訳ビュー。プロデュースタブ「投稿」タイル → ここに飛ぶ。
/// 件数と内訳はサーバ (`GET /users/:id/badges`) の値をそのまま出す。端末には持たない。
/// 上には、サーバから取った手応え (届いた Good・先週セトリが見られた数) を出す。
/// 個別履歴 (どのライブのセトリ編集か等) は将来の拡張 — 今は累計表示で割り切る。
struct MyContributionsView: View {
    /// サーバの編集の件数と種類別の内訳。未ログイン・未取得は nil。
    @State private var badges: UserBadges?
    @State private var feedbackStore = ContributionFeedbackStore.shared
    /// 公開された歌詞の曲名 (端末のマスタから引く)。
    @State private var lyricSongTitles: [String: String] = [:]

    var body: some View {
        ImasPage {
            ImasStatTile(systemImage: "square.and.pencil", value: "\(badges?.editCount ?? 0)", unit: "件",
                         label: "コミュニティへの投稿累計")

            feedbackSections

            if !breakdown.isEmpty {
                ImasSection("内訳", style: .small,
                            footer: "1 回の編集がいくつもの種類にまたがるときは、それぞれに数えます。") {
                    ImasStatGrid(columns: 2) {
                        ForEach(breakdown, id: \.type) { item in
                            ImasStatTile(systemImage: EditFeedFormat.recordTypeIcon(item.type), value: "\(item.count)",
                                         unit: "件", label: EditFeedFormat.recordTypeLabel(item.type))
                        }
                    }
                }
            }

            // セトリ編集・動画追加・タグ追加等、サーバに記録される「編集」の履歴確認・取り消し導線。
            // 投稿履歴(このビュー)と近い機能のため、内訳の直下にリンクとして置く。
            NavigationLink {
                MyEditsView()
            } label: {
                ImasEntryCard(systemImage: "arrow.uturn.backward.circle",
                              title: "自分の編集を確認・取り消す",
                              preview: "ライブ・楽曲・セトリの編集履歴")
            }
            .buttonStyle(.plain)

            ImasNote("アプリから入れた編集の件数です。取り消された編集も数えます。")
        }
        .navigationTitle("マイ投稿")
        .navigationBarTitleDisplayMode(.inline)
        .trackScreen("my_contributions")
        .task {
            badges = await BadgeService.shared.currentUserBadges(forceReload: true)
            await feedbackStore.refresh(force: true)
            let ids = feedbackStore.feedback?.lyrics?.published.map(\.songId) ?? []
            if !ids.isEmpty, let songs = try? await AppContainer.shared.songReading.songs(ids: ids) {
                lyricSongTitles = Dictionary(songs.map { ($0.id, $0.title) }, uniquingKeysWith: { a, _ in a })
            }
        }
        // 開いている間は NEW を見せたまま、閉じたときに既読にする。
        .onDisappear { feedbackStore.markGoodsSeen() }
    }

    /// 種類別の件数。多い順、同数は種類名の順。
    private var breakdown: [(type: String, count: Int)] {
        (badges?.categories ?? [:])
            .filter { $0.value > 0 }
            .map { (type: $0.key, count: $0.value) }
            .sorted { $0.count != $1.count ? $0.count > $1.count : $0.type < $1.type }
    }

    /// 届いた Good と、先週セトリが見られた数。ログインしていて取れたときだけ。
    @ViewBuilder
    private var feedbackSections: some View {
        if let feedback = feedbackStore.feedback {
            if let lyrics = feedback.lyrics, !lyrics.published.isEmpty {
                ImasSection("公開された歌詞", style: .small,
                            footer: "あなたの投稿で公開された歌詞と、いまの「ここ好き」の数です。") {
                    ImasCardList {
                        ForEach(Array(lyrics.published.enumerated()), id: \.element.id) { index, item in
                            if index > 0 { ImasRowDivider() }
                            ImasRecordRow(
                                systemImage: "text.quote", tone: .themed,
                                title: lyricSongTitles[item.songId] ?? "曲",
                                subtitle: "ここ好き \(item.likeCount)",
                                badges: item.isNew ? [ImasBadgeSpec(text: "NEW", kind: .new)] : []
                            )
                        }
                    }
                }
            }
            if feedback.setlistReach.viewers > 0 {
                ImasSection("先週の反響", style: .small,
                            footer: "あなたが入れたセトリの公演ページを見た人の数です。公演ごとに 1 人 1 回で数え、いくつも見た人はその数だけ数えます (のべ人数)。") {
                    ImasStatGrid(columns: 2) {
                        ImasStatTile(systemImage: "eye.fill", value: "\(feedback.setlistReach.viewers)", unit: "人",
                                     label: "セトリを見た人 (のべ)")
                        ImasStatTile(systemImage: "music.note.list", value: "\(feedback.setlistReach.shows)", unit: "公演",
                                     label: "見られた公演")
                    }
                }
            }
            if !feedback.goods.isEmpty {
                let times = EditFeedFormat.relativeTimes(feedback.goods.map {
                    ($0.batchId, Date(timeIntervalSince1970: TimeInterval($0.latestGoodAt) / 1000))
                })
                ImasSection("届いた Good", style: .small) {
                    ImasCardList {
                        ForEach(Array(feedback.goods.enumerated()), id: \.element.id) { index, good in
                            if index > 0 { ImasRowDivider() }
                            ImasRecordRow(
                                leading: .icon(EditFeedFormat.recordTypeIcon(good.recordType ?? ""), tone: .themed,
                                               seed: good.recordType),
                                title: good.summary ?? EditFeedFormat.recordTypeLabel(good.recordType ?? ""),
                                subtitle: "Good \(good.goodCount) ・ \(times[good.batchId] ?? "")",
                                badges: good.newGoodCount > 0 ? [ImasBadgeSpec(text: "NEW +\(good.newGoodCount)", kind: .new)] : []
                            )
                        }
                    }
                }
            }
        }
    }
}
