import SwiftUI

// =============================================================================
// 話し方と呼び方の部品 (docs/DESIGN_SYSTEM.md §6.15〜§6.17)
//
// ImasSpeechBoard    話し方の掲示板。一人称 (場面つき) と P の呼び方を暗い板に大きく。
// ImasCallPairHeader / ImasCallPairRow
//                    呼び方の 1 行。相手のアイコン・名前と「本人 → 相手」「相手 → 本人」を並べる。
//                    片方が未登録なら「—」(薄字)。見出しの行は列の題だけ。
// ImasCallMatrix     ユニットの呼称表。メンバー × (メンバー + P) のマス目。対角線は一人称 (暗い板の色)。
//                    マス・行頭を押すと組を選ぶ。人数が多いと横に流す。
// ImasCallPairCard   選んだ組の双方向の呼び方を、切り取り線で分けた紙に大きく。
//
// 使わない  「項目: 値」の 1 行 → ImasValueRow / 数の掲示板 → ImasBoard
// 色        墨・灰と掲示板の色だけ。アイドルの色はアイコンの輪にだけ出す。ぼかし・光は使わない。
// =============================================================================

// MARK: - 話し方の掲示板

struct ImasSpeechBoard: View {
    struct Form: Hashable {
        let text: String
        var scene: String? = nil
    }

    struct Column: Identifiable, Hashable {
        var id: String { title }
        let title: String
        let forms: [Form]
    }

    /// 1〜2 列 (一人称 / P の呼び方)。空の列は渡さない。
    let columns: [Column]

    var body: some View {
        HStack(alignment: .top, spacing: 0) {
            ForEach(Array(columns.enumerated()), id: \.element.id) { index, column in
                if index > 0 {
                    Rectangle().fill(DS.boardLine).frame(width: 1)
                }
                columnView(column)
            }
        }
        .fixedSize(horizontal: false, vertical: true)
        .background(DS.board, in: RoundedRectangle(cornerRadius: DS.rInner, style: .continuous))
    }

    private func columnView(_ column: Column) -> some View {
        VStack(alignment: .leading, spacing: DS.sp2) {
            Text(column.title)
                .font(.imasCaption2.weight(.bold))
                .tracking(1.2)
                .foregroundStyle(DS.boardDim)
            ForEach(Array(column.forms.enumerated()), id: \.offset) { index, form in
                VStack(alignment: .leading, spacing: DS.sp1) {
                    Text(form.text)
                        .font(index == 0 ? .imasHeading(22, weight: .heavy) : .imasHeading(16, weight: .bold))
                        .foregroundStyle(DS.boardInk)
                        .fixedSize(horizontal: false, vertical: true)
                    if let scene = form.scene {
                        Text(scene).font(.imasCaption2).foregroundStyle(DS.boardDim)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, DS.Space.rowH - 2)
        .padding(.vertical, DS.Space.gapLoose)
        .accessibilityElement(children: .combine)
    }
}

// MARK: - 呼び方の行

/// 呼び方の列の題 (「結華 → 相手」「相手 → 結華」)。列の幅は `ImasCallPairRow` と同じ。
struct ImasCallPairHeader: View {
    let outgoingTitle: String
    let incomingTitle: String

    @ScaledMetric(relativeTo: .footnote) private var avatarWidth: CGFloat = 30
    @ScaledMetric(relativeTo: .footnote) private var callWidth: CGFloat = 86

    var body: some View {
        HStack(spacing: DS.Space.gap) {
            Color.clear.frame(width: avatarWidth, height: 1)
            Spacer(minLength: 0)
            Text(outgoingTitle).frame(width: callWidth, alignment: .leading)
            Text(incomingTitle).frame(width: callWidth, alignment: .leading)
        }
        .font(.imasCaption2.weight(.bold))
        .foregroundStyle(DS.ink3)
        .lineLimit(1)
        .minimumScaleFactor(0.7)
        .padding(.horizontal, DS.Space.gapLoose)
        .padding(.vertical, DS.sp2 + 2)
        .accessibilityHidden(true)
    }
}

/// 呼び方の 1 行。相手のアイコン (写真か判子)・名前・本人 → 相手・相手 → 本人。
struct ImasCallPairRow: View {
    /// 相手の略称 (判子の文字)。
    let label: String
    let name: String
    var seed: String? = nil
    var brand: String? = nil
    var imageURL: URL? = nil
    let outgoing: String?
    let incoming: String?
    /// 読み上げで「三峰結華から」「三峰結華へ」と言うための本人の名前。
    let selfName: String

    @ScaledMetric(relativeTo: .footnote) private var avatarWidth: CGFloat = 30
    @ScaledMetric(relativeTo: .footnote) private var callWidth: CGFloat = 86

    var body: some View {
        HStack(spacing: DS.Space.gap) {
            ImasAvatar(label: label, seed: seed, brand: brand, size: avatarWidth, imageURL: imageURL, reservesPickRing: false)
            Text(name)
                .font(.imasFootnote)
                .foregroundStyle(DS.ink)
                .lineLimit(2)
                .frame(maxWidth: .infinity, alignment: .leading)
            call(outgoing, emphasized: true)
            call(incoming, emphasized: false)
        }
        .padding(.horizontal, DS.Space.gapLoose)
        .padding(.vertical, DS.Space.rowVCompact)
        .frame(minHeight: DS.Size.touch)
        .contentShape(Rectangle())
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(spoken)
    }

    private func call(_ text: String?, emphasized: Bool) -> some View {
        Text(text ?? "—")
            .font(.imasSubhead.weight(emphasized ? .heavy : .bold))
            .foregroundStyle(text == nil ? DS.ink3 : (emphasized ? DS.ink : DS.ink2))
            .lineLimit(2)
            .minimumScaleFactor(0.75)
            .frame(width: callWidth, alignment: .leading)
    }

    private var spoken: String {
        let out = outgoing.map { "\(selfName)から\(name)へは「\($0)」" } ?? "\(selfName)から\(name)への呼び方は未登録"
        let inc = incoming.map { "\(name)から\(selfName)へは「\($0)」" } ?? "\(name)から\(selfName)への呼び方は未登録"
        return "\(out)。\(inc)"
    }
}

// MARK: - 選んだ組

/// 選んだ組の双方向の呼び方。左右を切り取り線で分け、呼び方を大きく。
struct ImasCallPairCard: View {
    let forwardTitle: String
    let forward: String?
    let backwardTitle: String
    let backward: String?

    var body: some View {
        HStack(spacing: 0) {
            half(forwardTitle, forward)
            ImasPerforation(axis: .vertical, color: DS.perforation, lineWidth: 1.5)
            half(backwardTitle, backward)
        }
        .fixedSize(horizontal: false, vertical: true)
        .background(DS.surface, in: RoundedRectangle(cornerRadius: DS.rInner, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: DS.rInner, style: .continuous).strokeBorder(DS.line, lineWidth: 1))
    }

    private func half(_ title: String, _ value: String?) -> some View {
        VStack(alignment: .leading, spacing: DS.sp1 + 1) {
            Text(title).font(.imasCaption2.weight(.bold)).foregroundStyle(DS.ink3).lineLimit(1)
            Text(value ?? "—")
                .font(.imasHeading(22, weight: .heavy))
                .foregroundStyle(value == nil ? DS.ink3 : DS.ink)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(DS.Space.gapLoose)
        .accessibilityElement(children: .combine)
    }
}

// MARK: - ユニットの呼称表

struct ImasCallMatrix: View {
    struct Member: Identifiable, Hashable {
        let id: String
        /// 判子の文字 (略称)。
        let label: String
        let name: String
        var seed: String? = nil
        var imageURL: URL? = nil
    }

    enum CellKind: Hashable {
        case firstPerson, member, producer
    }

    struct Cell: Hashable {
        let kind: CellKind
        let text: String?
    }

    /// 選んだ組 (行 = 呼ぶ人、列 = 呼ばれる人。どちらも `members` の添字)。
    struct Selection: Hashable {
        var row: Int
        var column: Int
    }

    let members: [Member]
    /// `rows[i][j]` は members[i] が members[j] (j == members.count なら P) を呼ぶ呼び方。
    let rows: [[Cell]]
    @Binding var selection: Selection

    @ScaledMetric(relativeTo: .caption) private var headWidth: CGFloat = 44
    @ScaledMetric(relativeTo: .caption) private var minCellWidth: CGFloat = 50
    @ScaledMetric(relativeTo: .caption) private var cellHeight: CGFloat = 52
    @ScaledMetric(relativeTo: .caption) private var avatarSize: CGFloat = 26
    @State private var availableWidth: CGFloat = 0

    private var columnCount: Int { members.count + 1 }
    private var cellWidth: CGFloat {
        let fit = (availableWidth - headWidth) / CGFloat(max(columnCount, 1))
        return max(minCellWidth, fit.isFinite ? fit : minCellWidth)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: DS.Space.gap) {
            ScrollView(.horizontal, showsIndicators: false) {
                grid
            }
            .scrollDisabled(cellWidth * CGFloat(columnCount) + headWidth <= availableWidth + 0.5)
            .background(DS.surface, in: RoundedRectangle(cornerRadius: DS.rInner, style: .continuous))
            .clipShape(RoundedRectangle(cornerRadius: DS.rInner, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: DS.rInner, style: .continuous).strokeBorder(DS.line, lineWidth: 1))
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { availableWidth = $0 }
            legend
        }
        .sensoryFeedback(.selection, trigger: selection)
    }

    private var grid: some View {
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                Color.clear.frame(width: headWidth, height: 1)
                ForEach(members) { m in
                    avatar(m, size: avatarSize).frame(width: cellWidth)
                }
                Text("P")
                    .font(.imasCaption.weight(.heavy))
                    .foregroundStyle(DS.ink)
                    .frame(width: cellWidth)
                    .accessibilityLabel("プロデューサー")
            }
            .padding(.vertical, DS.sp3)
            .overlay(alignment: .bottom) { Rectangle().fill(DS.ink).frame(height: 1.5) }

            ForEach(Array(members.enumerated()), id: \.element.id) { i, m in
                HStack(spacing: 0) {
                    Button { pickRow(i) } label: {
                        avatar(m, size: avatarSize + 4)
                            .frame(width: headWidth, height: cellHeight)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("\(m.name)の行")
                    ForEach(Array(rows[i].enumerated()), id: \.offset) { j, cell in
                        cellView(cell, row: i, column: j)
                    }
                }
                .background(i == selection.row ? DS.fill : Color.clear)
                .overlay(alignment: .bottom) { Rectangle().fill(DS.sep).frame(height: 1) }
            }
        }
    }

    private func avatar(_ m: Member, size: CGFloat) -> some View {
        ImasAvatar(label: m.label, seed: m.seed, size: size, imageURL: m.imageURL, reservesPickRing: false)
    }

    @ViewBuilder
    private func cellView(_ cell: Cell, row: Int, column: Int) -> some View {
        let isSelected = selection == Selection(row: row, column: column)
        let selectable = cell.kind == .member
        let label = Text(cell.text ?? "—")
            .font(.imasScaled(11, weight: cell.kind == .firstPerson || isSelected ? .heavy : .medium))
            .foregroundStyle(cell.kind == .firstPerson ? DS.boardInk : (cell.text == nil ? DS.ink3 : DS.ink))
            .lineLimit(3)
            .minimumScaleFactor(0.75)
            .multilineTextAlignment(.center)
            .padding(.horizontal, DS.sp1)
            .frame(width: cellWidth, height: cellHeight)
            .background {
                if cell.kind == .firstPerson {
                    Rectangle().fill(DS.board)
                } else if isSelected {
                    Rectangle().fill(DS.surface)
                }
            }
            .overlay {
                if isSelected { Rectangle().strokeBorder(DS.ink, lineWidth: 1.5) }
            }
            .overlay(alignment: .leading) { Rectangle().fill(DS.sep).frame(width: 1) }
        if selectable {
            Button {
                selection = Selection(row: row, column: column)
            } label: { label.contentShape(Rectangle()) }
                .buttonStyle(.plain)
                .accessibilityLabel(spoken(cell, row: row, column: column))
                .accessibilityAddTraits(isSelected ? .isSelected : [])
        } else {
            label.accessibilityLabel(spoken(cell, row: row, column: column))
        }
    }

    private func spoken(_ cell: Cell, row: Int, column: Int) -> String {
        let from = members[row].name
        let value = cell.text ?? "未登録"
        switch cell.kind {
        case .firstPerson: return "\(from)の一人称 \(value)"
        case .producer: return "\(from)からプロデューサーへ \(value)"
        case .member: return "\(from)から\(members[column].name)へ \(value)"
        }
    }

    /// 行頭を押したら、その人の行に移る。列はそのままにし、自分の列なら隣の人へずらす。
    private func pickRow(_ row: Int) {
        var column = selection.column
        if column == row { column = (row + 1) % max(members.count, 1) }
        selection = Selection(row: row, column: column)
    }

    private var legend: some View {
        HStack(spacing: DS.Space.gapLoose) {
            HStack(spacing: DS.sp2) {
                Rectangle().fill(DS.board).frame(width: 12, height: 12)
                Text("一人称")
            }
            HStack(spacing: DS.sp2) {
                Rectangle().strokeBorder(DS.ink, lineWidth: 1.5).frame(width: 12, height: 12)
                Text("選んだ組")
            }
        }
        .font(.imasCaption2)
        .foregroundStyle(DS.ink2)
        .accessibilityHidden(true)
    }
}
