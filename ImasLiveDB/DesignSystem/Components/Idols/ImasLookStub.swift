import SwiftUI

// =============================================================================
// ImasLookStub 見た目の半券 (docs/DESIGN_SYSTEM.md §6.14)
//
// 用途      アイドル詳細のプロフィールで、髪色・髪型など「見た目」の項目を 1 枚の紙で見せる。
// 使わない  ほかの「項目: 値」→ ImasValueRow / 色そのものを選ぶ → ImasSwatch・ImasColorPicker
// 構成      [左の半券: 髪色の見本 (フラットな 1 色。2 色目があれば左右に硬く分ける) + 色名]
//           ┆ 切り取り線 ┆
//           [右: 項目名 + 値の格子。押せる値 (髪型) は四角い枠の札 + ›]
//           版 (基本 / セカンドヘア …) の切り替えは呼び出し側が上に `ImasSegmented` で置く。
// 状態      見本なし (髪色が無い子は半券を出さず右だけ) / 押せる値あり・なし
// 色        見本の色は中身そのもの (コアの `hair_color_hex`)。ほかは墨と灰だけ。ぼかし・光は使わない。
// =============================================================================

struct ImasLookStub: View {
    struct Swatch: Hashable {
        let hex: String
        let name: String
    }

    struct Item: Identifiable, Hashable {
        var id: String { label }
        let label: String
        let values: [String]
        /// 値を押すと別の画面へ行ける (枠の札 + › で出す)。
        let opensList: Bool
    }

    let swatches: [Swatch]
    /// 見本の下の色名 (「髪色 黒／金」)。
    let swatchCaption: String
    let items: [Item]
    /// 押せる値を押したとき (項目, 値)。
    var onSelect: ((Item, String) -> Void)? = nil

    @ScaledMetric(relativeTo: .footnote) private var stubWidth: CGFloat = 84
    @ScaledMetric(relativeTo: .footnote) private var labelWidth: CGFloat = 52

    private var hasStub: Bool { !swatches.isEmpty }

    var body: some View {
        HStack(spacing: 0) {
            if hasStub {
                stub
                    .frame(width: stubWidth)
                ImasPerforation(axis: .vertical, color: DS.perforation, lineWidth: 1.5)
            }
            Grid(alignment: .leadingFirstTextBaseline, horizontalSpacing: DS.Space.gap, verticalSpacing: DS.Space.gap) {
                ForEach(items) { item in
                    GridRow {
                        Text(item.label)
                            .imasText(.meta, color: DS.ink2)
                            .frame(width: labelWidth, alignment: .leading)
                        values(item)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
            .padding(.horizontal, DS.Space.gapLoose)
            .padding(.vertical, DS.Space.gapLoose)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .fixedSize(horizontal: false, vertical: true)
        .background(DS.surface, in: shape)
        .overlay(shape.stroke(DS.line, lineWidth: 1))
    }

    private var shape: ImasTicketShape {
        ImasTicketShape(cornerRadius: DS.rInner, cut: hasStub ? .verticalFromLeading(stubWidth) : .none, notchRadius: 7)
    }

    /// 左の半券。見本は平らな色の面 (2 色なら左右に分ける)、下に色名。
    private var stub: some View {
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                ForEach(swatches, id: \.self) { swatch in
                    Rectangle().fill(Color(hexString: swatch.hex))
                }
            }
            .frame(maxHeight: .infinity)
            .accessibilityHidden(true)
            Text(swatchCaption)
                .font(.imasCaption2.weight(.bold))
                .foregroundStyle(DS.ink2)
                .lineLimit(2)
                .minimumScaleFactor(0.8)
                .multilineTextAlignment(.center)
                .padding(.horizontal, DS.Space.gapTight)
                .padding(.vertical, DS.sp2 + 1)
        }
        .clipShape(UnevenRoundedRectangle(topLeadingRadius: DS.rInner, bottomLeadingRadius: DS.rInner, style: .continuous))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(swatchCaption)
    }

    @ViewBuilder
    private func values(_ item: Item) -> some View {
        if item.opensList, let onSelect {
            FlowLayout(spacing: DS.sp2) {
                ForEach(item.values, id: \.self) { value in
                    Button { onSelect(item, value) } label: { tag(value) }
                        .buttonStyle(.imasPress)
                        .accessibilityLabel("\(value)のアイドル")
                }
            }
        } else {
            Text(item.values.joined(separator: "・"))
                .font(.imasFootnote.weight(.bold))
                .foregroundStyle(DS.ink)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    /// 押せる値の札。四角い枠 + ›。指の当たりは札の外まで広げる。
    private func tag(_ value: String) -> some View {
        HStack(spacing: 2) {
            Text(value)
            Image(systemName: "chevron.right").font(.imasScaled(9, weight: .bold))
        }
        .font(.imasCaption.weight(.bold))
        .foregroundStyle(DS.ink)
        .padding(.horizontal, DS.sp3 - 1)
        .padding(.vertical, DS.sp1)
        .overlay(RoundedRectangle(cornerRadius: DS.rTag / 2, style: .continuous).strokeBorder(DS.ink, lineWidth: 1.5))
        .padding(.vertical, DS.sp2)
        .contentShape(Rectangle())
    }
}
