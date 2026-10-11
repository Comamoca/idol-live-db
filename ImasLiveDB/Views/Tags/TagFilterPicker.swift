import SwiftUI

/// 曲一覧をコミュニティタグで絞り込むためのタグ**複数選択**シート。
/// 「完了」で onDone(選択タグ配列) を呼ぶ。複数選択時は AND (全タグを含む曲) で絞る想定。
struct TagFilterPicker: View {
    @Environment(\.dismiss) private var dismiss
    let onDone: ([CommunityTag]) -> Void

    /// 候補の全件 (人気順)。検索欄は手元で絞る (`TagSearchPool`)。
    @State private var pool = TagSearchPool()
    /// 選択中タグ。検索で一覧から消えても保持するため id ではなくオブジェクトで持つ。
    @State private var selected: [CommunityTag]
    @State private var query = ""
    @State private var isLoading = true
    /// 検索していないときの並べ方 (人気タグランキング / カテゴリ別)。
    @State private var layout: TagListLayout = .flat

    init(initialSelection: [CommunityTag], onDone: @escaping ([CommunityTag]) -> Void) {
        self.onDone = onDone
        _selected = State(initialValue: initialSelection)
    }

    private var trimmedQuery: String { query.trimmingCharacters(in: .whitespaces) }
    private var candidates: [CommunityTag] { pool.matching(trimmedQuery) }

    private func isSelected(_ tag: CommunityTag) -> Bool {
        selected.contains { $0.id == tag.id }
    }
    private func toggle(_ tag: CommunityTag) {
        if let idx = selected.firstIndex(where: { $0.id == tag.id }) {
            selected.remove(at: idx)
        } else {
            selected.append(tag)
        }
    }

    var body: some View {
        NavigationStack {
            List {
                if !selected.isEmpty {
                    ImasListSection("選択中 (\(selected.count)) — すべてを含む曲に絞り込み") {
                        ImasChipRow {
                            ForEach(selected) { tag in
                                ImasRemovableChip(text: tag.name, seed: tag.color?.rawValue) { toggle(tag) }
                            }
                        }
                        .padding(.vertical, DS.Space.gapTight)
                    }
                    .listRowInsets(EdgeInsets())
                }
                if trimmedQuery.isEmpty {
                    ImasTabs(options: TagListLayout.allCases, selection: $layout) { $0.label }
                        .listRowInsets(EdgeInsets(top: 0, leading: DS.Space.screen, bottom: 0, trailing: DS.Space.screen))
                        .listRowBackground(Color.clear)
                        .listRowSeparator(.hidden)
                }
                if isLoading {
                    ImasInlineLoading()
                        .listRowBackground(Color.clear)
                } else if candidates.isEmpty {
                    ImasEmptyState(.empty, title: "タグがありません")
                        .listRowBackground(DS.surface)
                } else if trimmedQuery.isEmpty && layout == .byCategory {
                    // カテゴリ (ムード・シーン …) ごとに見出しを分ける。中は人気順。
                    ForEach(candidates.groupedByCategory { songTagCategoryGroups(tags: $0) }) { group in
                        ImasListSection(group.label, count: "\(group.tags.count)") {
                            ForEach(group.tags) { tag in tagRow(tag, rank: nil) }
                        }
                    }
                } else {
                    ImasListSection(trimmedQuery.isEmpty ? "人気タグランキング" : "検索結果") {
                        ForEach(Array(candidates.enumerated()), id: \.element.id) { idx, tag in
                            // 検索していない時は人気順そのものなので順位バッジを出す。
                            tagRow(tag, rank: trimmedQuery.isEmpty ? idx + 1 : nil)
                        }
                    }
                }
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .background(DS.bg)
            .searchable(text: $query, prompt: "タグ名で検索")
            .navigationTitle("タグで絞り込み")
            .navigationBarTitleDisplayMode(.inline)
            .imasSheetToolbar(.select(onCancel: { dismiss() }, onFinish: {
                AppAnalytics.tap("tag_filter.done")
                onDone(selected)
                dismiss()
            }))
            .task { await load() }
            .trackScreen("tag_filter")
        }
    }

    /// タグ候補 1 行。`rank` があれば順位バッジを出す。
    private func tagRow(_ tag: CommunityTag, rank: Int?) -> some View {
        Button {
            AppAnalytics.tap("tag_filter.toggle_tag")
            toggle(tag)
        } label: {
            HStack(spacing: DS.Space.gapTight) {
                if let rank { ImasRankBadge(rank: rank) }
                ImasRow(
                    title: tag.name,
                    leading: tag.color.map { .custom(AnyView(ImasSwatch(hex: $0.rawValue, size: .small)), width: 16) } ?? .none,
                    selection: ImasRowSelection(isOn: isSelected(tag), seed: tag.color?.rawValue),
                    trailing: (tag.totalUses ?? 0) > 0 ? .value("\(tag.totalUses ?? 0)曲") : .none,
                    density: .compact
                )
            }
        }
        .buttonStyle(.imasRow)
        .sensoryFeedback(.selection, trigger: isSelected(tag))
    }

    /// 全件を 1 回だけ取る。検索欄の絞り込みは手元 (`pool.matching`) なので、打鍵ごとには取り直さない
    /// (古い語の応答が後から届いて、消した後の一覧を上書きすることが無い)。
    private func load() async {
        isLoading = true
        defer { isLoading = false }
        pool.replace((try? await AppContainer.shared.communityTagReading.tags(search: "", category: "", sort: "popular")) ?? [])
    }
}
