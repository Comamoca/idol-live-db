import SwiftUI

/// アイドルの呼び方の全員。1 行に「本人 → 相手」と「相手 → 本人」を並べ、片方が無ければ「—」。
/// ユニットの仲間を先頭に、ブランドごとにまとめる。名前・読み・呼び方で探せて、向きで絞れる。
///
/// まとめ方・並び・検索の照合・向きの絞り込み・コピーの文面はコア (`idol_call_groups`)。
/// 行を押すとそのアイドルへ、長押しのメニューで呼び方をコピーできる。
struct IdolCallListView: View {
    let idol: Idol
    var navigate: ((DetailDestination) -> Void)? = nil

    @State private var query = ""
    @State private var direction: CallDirection = .both
    @State private var groups: [IdolCallGroup] = []
    @State private var others: [String: Idol] = [:]
    @State private var hasLoaded = false
    @State private var imageService = CustomImageService.shared

    private let idolReading: any IdolReading = AppContainer.shared.idolReading

    private var selfLabel: String { idol.shortName }

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: DS.Space.gapLoose, pinnedViews: [.sectionHeaders]) {
                Section {
                    if hasLoaded && groups.isEmpty {
                        ImasEmptyState(
                            systemImage: "text.bubble",
                            title: query.isEmpty ? "呼び方がありません" : "見つかりません",
                            message: query.isEmpty ? "この向きの呼び方はまだ登録されていません。" : "名前・読み・呼び方を変えてみてください。"
                        )
                    }
                    ForEach(groups, id: \.title) { group in
                        VStack(alignment: .leading, spacing: DS.Space.header) {
                            ImasSectionHeader(group.title, count: "\(group.pairs.count)", style: .small)
                            ImasCardList {
                                ForEach(Array(group.pairs.enumerated()), id: \.element.otherId) { index, pair in
                                    if index > 0 { ImasRowDivider(inset: DS.Space.gapLoose) }
                                    row(pair)
                                }
                            }
                        }
                    }
                    if !groups.isEmpty {
                        ImasNote("呼び方はニコニコ大百科の呼称表・im@sparql (MIT License) などから。場面や時期で変わる呼び方は「／」で並べています。")
                    }
                } header: {
                    controls
                }
            }
            .padding(.horizontal, DS.Space.screen)
            .padding(.bottom, DS.sp7)
        }
        .background(DS.bg)
        .scrollContentBackground(.hidden)
        .scrollDismissesKeyboard(.immediately)
        .navigationTitle("\(idol.name)の呼び方")
        .navigationBarTitleDisplayMode(.inline)
        .task(id: "\(query)|\(String(describing: direction))") { await load() }
        .trackScreen("idol_calls")
    }

    /// 検索・向きの切り替え・列の題。上に貼り付ける。
    private var controls: some View {
        VStack(spacing: DS.Space.gap) {
            ImasSearchField(prompt: "名前・呼び方で探す", text: $query)
            ImasSegmented(options: [CallDirection.both, .outgoing, .incoming], selection: $direction) { d in
                switch d {
                case .both: "両方"
                case .outgoing: "\(selfLabel)が呼ぶ"
                case .incoming: "\(selfLabel)を呼ぶ"
                }
            }
            ImasCallPairHeader(outgoingTitle: "\(selfLabel) → 相手", incomingTitle: "相手 → \(selfLabel)")
        }
        .padding(.top, DS.Space.gap)
        .background(DS.bg)
    }

    private func row(_ pair: IdolCallPair) -> some View {
        let other = others[pair.otherId]
        return Button {
            if let other { navigate?(.idol(other)) }
        } label: {
            ImasCallPairRow(
                label: other?.shortName ?? pair.otherName,
                name: pair.otherName,
                seed: other?.color,
                imageURL: imageService.imageURL(for: pair.otherId),
                outgoing: pair.outgoing,
                incoming: pair.incoming,
                selfName: idol.name
            )
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button {
                UIPasteboard.general.string = pair.copyText
            } label: { Label("呼び方をコピー", systemImage: "doc.on.doc") }
            if let other {
                Button { navigate?(.idol(other)) } label: { Label("\(other.name)を見る", systemImage: "person") }
            }
        }
    }

    private func load() async {
        let loaded = (try? await idolReading.callGroups(idolId: idol.id, query: query, direction: direction)) ?? []
        // 打鍵ごとに走るので、追い越された古い結果で上書きしない。
        guard !Task.isCancelled else { return }
        let missing = Set(loaded.flatMap { $0.pairs.map(\.otherId) }).subtracting(others.keys)
        if !missing.isEmpty, let fetched = try? await idolReading.idols(ids: Array(missing)) {
            for other in fetched { others[other.id] = other }
        }
        guard !Task.isCancelled else { return }
        groups = loaded
        hasLoaded = true
    }
}
