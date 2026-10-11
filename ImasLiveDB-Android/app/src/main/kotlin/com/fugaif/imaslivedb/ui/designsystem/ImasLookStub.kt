package com.fugaif.imaslivedb.ui.designsystem

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.fugaif.imaslivedb.ui.theme.DS
import com.fugaif.imaslivedb.ui.theme.ImasTextRole
import com.fugaif.imaslivedb.ui.theme.hexToColor
import com.fugaif.imaslivedb.ui.theme.imasPress

// =============================================================================
// ImasLookStub 見た目の半券 (docs/DESIGN_SYSTEM.md §6.14)。iOS `ImasLookStub.swift` の移植。
//
// 用途      アイドル詳細のプロフィールで、髪色・髪型など「見た目」の項目を 1 枚の紙で見せる。
// 使わない  ほかの「項目: 値」→ ImasLabeledRow / 色そのものを選ぶ → ImasColorPicker
// 構成      [左の半券: 髪色の見本 (フラットな 1 色。2 色目があれば左右に硬く分ける) + 色名]
//           ┆ 切り取り線 ┆
//           [右: 項目名 + 値の格子。押せる値 (髪型) は四角い枠の札 + ›]
//           版 (基本 / セカンドヘア …) の切り替えは呼び出し側が上に [ImasSegmented] で置く。
// 状態      見本なし (髪色が無い子は半券を出さず右だけ) / 押せる値あり・なし
// 色        見本の色は中身そのもの (コアの hair_color_hex)。ほかは墨と灰だけ。ぼかし・光は使わない。
// =============================================================================

/** 髪色の見本 1 色 (iOS `ImasLookStub.Swatch`)。 */
@Immutable
data class ImasLookSwatch(val hex: String, val name: String)

/** 見た目の項目 1 つ (iOS `ImasLookStub.Item`)。 */
@Immutable
data class ImasLookItemSpec(
    val label: String,
    val values: List<String>,
    /** 値を押すと別の画面へ行ける (枠の札 + › で出す)。 */
    val opensList: Boolean
)

@Composable
fun ImasLookStub(
    swatches: List<ImasLookSwatch>,
    swatchCaption: String,
    items: List<ImasLookItemSpec>,
    modifier: Modifier = Modifier,
    onSelect: ((ImasLookItemSpec, String) -> Unit)? = null
) {
    val stubWidth = with(LocalDensity.current) { 84.sp.toDp() }
    val labelWidth = with(LocalDensity.current) { 52.sp.toDp() }
    val hasStub = swatches.isNotEmpty()
    val shape = ImasTicketShape(
        cornerRadius = DS.rInner,
        cut = if (hasStub) ImasTicketShape.Cut.VerticalFromLeading(stubWidth) else ImasTicketShape.Cut.None,
        notchRadius = 7.dp
    )

    Row(
        modifier
            .fillMaxWidth()
            // 半券の見本は中身の高さいっぱいに塗る (固有の高さを持たない面なので、行の高さを中身に合わせる)。
            .height(IntrinsicSize.Min)
            .background(DS.surface, shape)
            .border(1.dp, DS.line, shape)
            .clip(shape)
    ) {
        if (hasStub) {
            Stub(swatches, swatchCaption, stubWidth)
            ImasPerforation(axis = ImasPerforationAxis.VERTICAL, color = DS.perforation, lineWidth = 1.5.dp, modifier = Modifier.fillMaxHeight())
        }
        Column(
            Modifier
                .weight(1f)
                .padding(horizontal = DS.Space.gapLoose, vertical = DS.Space.gapLoose),
            verticalArrangement = Arrangement.spacedBy(DS.Space.gap)
        ) {
            items.forEach { item ->
                Row(horizontalArrangement = Arrangement.spacedBy(DS.Space.gap)) {
                    Text(
                        item.label,
                        style = ImasTextRole.META.style,
                        color = DS.ink2,
                        modifier = Modifier.width(labelWidth)
                    )
                    Values(item, onSelect)
                }
            }
        }
    }
}

/** 左の半券。見本は平らな色の面 (2 色なら左右に分ける)、下に色名。 */
@Composable
private fun Stub(swatches: List<ImasLookSwatch>, swatchCaption: String, stubWidth: androidx.compose.ui.unit.Dp) {
    Column(
        Modifier
            .width(stubWidth)
            .fillMaxHeight()
            .semantics { contentDescription = swatchCaption },
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Row(Modifier.weight(1f).fillMaxWidth().clearAndSetSemantics { }) {
            swatches.forEach { swatch ->
                Box(Modifier.weight(1f).fillMaxHeight().background(hexToColor(swatch.hex)))
            }
        }
        Text(
            swatchCaption,
            style = ImasTextRole.BADGE.style,
            color = DS.ink2,
            textAlign = TextAlign.Center,
            maxLines = 2,
            modifier = Modifier.padding(horizontal = DS.Space.gapTight, vertical = DS.sp2 + 1.dp)
        )
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Values(item: ImasLookItemSpec, onSelect: ((ImasLookItemSpec, String) -> Unit)?) {
    if (item.opensList && onSelect != null) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(DS.sp2), verticalArrangement = Arrangement.spacedBy(DS.sp2)) {
            item.values.forEach { value -> Tag(value) { onSelect(item, value) } }
        }
    } else {
        Text(
            item.values.joinToString("・"),
            style = ImasTextRole.NOTE.style.copy(fontWeight = FontWeight.Bold),
            color = DS.ink
        )
    }
}

/** 押せる値の札。四角い枠 + ›。 */
@Composable
private fun Tag(value: String, onClick: () -> Unit) {
    Row(
        Modifier
            .imasPress(onClickLabel = "${value}のアイドル", onClick = onClick)
            .border(1.5.dp, DS.ink, RoundedCornerShape(DS.rTag / 2))
            .padding(horizontal = DS.sp3 - 1.dp, vertical = DS.sp1),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(2.dp)
    ) {
        Text(value, style = ImasTextRole.BADGE.style, color = DS.ink)
        Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null, tint = DS.ink, modifier = Modifier.size(10.dp))
    }
}
