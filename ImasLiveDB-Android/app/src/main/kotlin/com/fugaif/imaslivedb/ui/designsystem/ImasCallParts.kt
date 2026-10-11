package com.fugaif.imaslivedb.ui.designsystem

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.fugaif.imaslivedb.ui.theme.DS
import com.fugaif.imaslivedb.ui.theme.ImasTextRole
import com.fugaif.imaslivedb.ui.theme.ImasType
import com.fugaif.imaslivedb.ui.theme.imasPress
import com.fugaif.imaslivedb.ui.theme.rememberImasHaptics

// =============================================================================
// 話し方と呼び方の部品 (docs/DESIGN_SYSTEM.md §6.15〜§6.17)。iOS `ImasCallParts.swift` の移植。
//
// ImasSpeechBoard    話し方の掲示板。一人称 (場面つき) と P の呼び方を暗い板に大きく。
// ImasCallPairHeader / ImasCallPairRow
//                    呼び方の 1 行。相手のアイコン・名前と「本人 → 相手」「相手 → 本人」を並べる。
//                    片方が未登録なら「—」(薄字)。見出しの行は列の題だけ。
// ImasCallMatrix     ユニットの呼称表。メンバー × (メンバー + P) のマス目。対角線は一人称 (暗い板の色)。
//                    マス・行頭を押すと組を選ぶ。人数が多いと横に流す。
// ImasCallPairCard   選んだ組の双方向の呼び方を、切り取り線で分けた紙に大きく。
//
// 使わない  「項目: 値」の 1 行 → ImasLabeledRow / 数の掲示板 → ImasBoard
// 色        墨・灰と掲示板の色だけ。アイドルの色はアイコンの輪にだけ出す。ぼかし・光は使わない。
// =============================================================================

// MARK: - 話し方の掲示板

/** 話し方 1 つ (場面つき)。iOS `ImasSpeechBoard.Form`。 */
@Immutable
data class ImasSpeechForm(val text: String, val scene: String? = null)

/** 話し方の列 (「一人称」「Pの呼び方」)。iOS `ImasSpeechBoard.Column`。 */
@Immutable
data class ImasSpeechColumn(val title: String, val forms: List<ImasSpeechForm>)

@Composable
fun ImasSpeechBoard(columns: List<ImasSpeechColumn>, modifier: Modifier = Modifier) {
    Row(
        modifier
            .fillMaxWidth()
            .background(DS.board, RoundedCornerShape(DS.rInner)),
        verticalAlignment = Alignment.Top
    ) {
        columns.forEachIndexed { index, column ->
            if (index > 0) Box(Modifier.fillMaxHeight().width(1.dp).background(DS.boardLine))
            SpeechColumnView(column, Modifier.weight(1f))
        }
    }
}

@Composable
private fun SpeechColumnView(column: ImasSpeechColumn, modifier: Modifier = Modifier) {
    Column(
        modifier.padding(horizontal = DS.Space.rowH - 2.dp, vertical = DS.Space.gapLoose),
        verticalArrangement = Arrangement.spacedBy(DS.sp2)
    ) {
        Text(
            column.title,
            style = ImasTextRole.BADGE.style.copy(letterSpacing = 1.2.sp),
            color = DS.boardDim
        )
        column.forms.forEachIndexed { index, form ->
            Column(verticalArrangement = Arrangement.spacedBy(DS.sp1)) {
                Text(
                    form.text,
                    style = ImasType.heading(if (index == 0) 22.sp else 16.sp, if (index == 0) FontWeight.Black else FontWeight.Bold),
                    color = DS.boardInk
                )
                form.scene?.let { Text(it, style = ImasTextRole.BADGE.style, color = DS.boardDim) }
            }
        }
    }
}

// MARK: - 呼び方の行

private val CallAvatarWidth = 30.dp
private val CallWidth = 86.dp

/** 呼び方の列の題 (「結華 → 相手」「相手 → 結華」)。列の幅は [ImasCallPairRow] と同じ。 */
@Composable
fun ImasCallPairHeader(outgoingTitle: String, incomingTitle: String, modifier: Modifier = Modifier) {
    Row(
        modifier
            .fillMaxWidth()
            .padding(horizontal = DS.Space.gapLoose, vertical = DS.sp2 + 2.dp)
            .clearAndSetSemantics { },
        horizontalArrangement = Arrangement.spacedBy(DS.Space.gap),
        verticalAlignment = Alignment.CenterVertically
    ) {
        Box(Modifier.width(CallAvatarWidth))
        Box(Modifier.weight(1f))
        Text(outgoingTitle, style = ImasTextRole.BADGE.style, color = DS.ink3, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.width(CallWidth))
        Text(incomingTitle, style = ImasTextRole.BADGE.style, color = DS.ink3, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.width(CallWidth))
    }
}

/** 呼び方の 1 行。相手のアイコン (写真か判子)・名前・本人 → 相手・相手 → 本人。 */
@Composable
fun ImasCallPairRow(
    label: String,
    name: String,
    outgoing: String?,
    incoming: String?,
    /** 読み上げで「三峰結華から」「三峰結華へ」と言うための本人の名前。 */
    selfName: String,
    modifier: Modifier = Modifier,
    seed: String? = null,
    brand: String? = null,
    imageUrl: String? = null,
    entityId: String? = null
) {
    val spoken = buildString {
        append(outgoing?.let { "${selfName}から${name}へは「$it」" } ?: "${selfName}から${name}への呼び方は未登録")
        append("。")
        append(incoming?.let { "${name}から${selfName}へは「$it」" } ?: "${name}から${selfName}への呼び方は未登録")
    }
    Row(
        modifier
            .fillMaxWidth()
            .padding(horizontal = DS.Space.gapLoose, vertical = DS.Space.rowVCompact)
            .heightIn(min = DS.Size.touch)
            .semantics(mergeDescendants = true) { contentDescription = spoken },
        horizontalArrangement = Arrangement.spacedBy(DS.Space.gap),
        verticalAlignment = Alignment.CenterVertically
    ) {
        ImasAvatar(label = label, seed = seed, brand = brand, size = CallAvatarWidth, imageUrl = imageUrl, entityId = entityId, reservesPickRing = false)
        Text(
            name,
            style = ImasTextRole.NOTE.style,
            color = DS.ink,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f)
        )
        CallValue(outgoing, emphasized = true)
        CallValue(incoming, emphasized = false)
    }
}

@Composable
private fun CallValue(text: String?, emphasized: Boolean) {
    Text(
        text ?: "—",
        style = ImasTextRole.NOTE.style.copy(fontSize = 15.sp, fontWeight = if (emphasized) FontWeight.Black else FontWeight.Bold),
        color = if (text == null) DS.ink3 else if (emphasized) DS.ink else DS.ink2,
        maxLines = 2,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier.width(CallWidth)
    )
}

// MARK: - 選んだ組

/** 選んだ組の双方向の呼び方。左右を切り取り線で分け、呼び方を大きく。 */
@Composable
fun ImasCallPairCard(
    forwardTitle: String,
    forward: String?,
    backwardTitle: String,
    backward: String?,
    modifier: Modifier = Modifier
) {
    val shape = RoundedCornerShape(DS.rInner)
    Row(
        modifier
            .fillMaxWidth()
            .background(DS.surface, shape)
            .border(1.dp, DS.line, shape)
            .clip(shape)
    ) {
        CallPairHalf(forwardTitle, forward, Modifier.weight(1f))
        ImasPerforation(axis = ImasPerforationAxis.VERTICAL, color = DS.perforation, lineWidth = 1.5.dp, modifier = Modifier.fillMaxHeight())
        CallPairHalf(backwardTitle, backward, Modifier.weight(1f))
    }
}

@Composable
private fun CallPairHalf(title: String, value: String?, modifier: Modifier = Modifier) {
    Column(
        modifier.padding(DS.Space.gapLoose),
        verticalArrangement = Arrangement.spacedBy(DS.sp1 + 1.dp)
    ) {
        Text(title, style = ImasTextRole.BADGE.style, color = DS.ink3, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(
            value ?: "—",
            style = ImasType.heading(22.sp, FontWeight.Black),
            color = if (value == null) DS.ink3 else DS.ink
        )
    }
}

// MARK: - ユニットの呼称表

/** 呼称表のマスの種類。iOS `ImasCallMatrix.CellKind`。 */
enum class ImasCallMatrixCellKind { FIRST_PERSON, MEMBER, PRODUCER }

/** 呼称表のメンバー (iOS `ImasCallMatrix.Member`)。 */
@Immutable
data class ImasCallMatrixMember(
    val id: String,
    /** 判子の文字 (略称)。 */
    val label: String,
    val name: String,
    val seed: String? = null,
    val imageUrl: String? = null
)

/** 呼称表のマス (iOS `ImasCallMatrix.Cell`)。 */
@Immutable
data class ImasCallMatrixCell(val kind: ImasCallMatrixCellKind, val text: String?)

/** 呼称表の選んだ組 (行 = 呼ぶ人、列 = 呼ばれる人。どちらも members の添字)。iOS `ImasCallMatrix.Selection`。 */
@Immutable
data class ImasCallMatrixSelection(val row: Int, val column: Int)

private val MatrixSelectionSaver: Saver<ImasCallMatrixSelection, *> = listSaver(
    save = { listOf(it.row, it.column) },
    restore = { ImasCallMatrixSelection(it[0], it[1]) }
)

/** [ImasCallMatrixSelection] を画面回転等を跨いで保存するための Saver (呼び出し側が rememberSaveable に渡す)。 */
fun imasCallMatrixSelectionSaver(): Saver<ImasCallMatrixSelection, *> = MatrixSelectionSaver

private val MatrixHeadWidth = 44.dp
private val MatrixMinCellWidth = 50.dp
private val MatrixCellHeight = 52.dp
private val MatrixAvatarSize = 26.dp

/**
 * ユニットの呼称表。メンバー × (メンバー + P) のマス目 (iOS `ImasCallMatrix`)。
 *
 * 選択が変わった触覚は表全体で 1 回 (マスごとに鳴らさない)。
 */
@Composable
fun ImasCallMatrix(
    members: List<ImasCallMatrixMember>,
    rows: List<List<ImasCallMatrixCell>>,
    selection: ImasCallMatrixSelection,
    onSelectionChange: (ImasCallMatrixSelection) -> Unit,
    modifier: Modifier = Modifier
) {
    val haptics = rememberImasHaptics()
    var lastSelection by remember { mutableStateOf(selection) }
    LaunchedEffect(selection) {
        if (selection != lastSelection) {
            lastSelection = selection
            haptics.selection()
        }
    }

    val columnCount = members.size + 1
    Column(modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(DS.Space.gap)) {
        BoxWithConstraints {
            val minTotal = MatrixHeadWidth + MatrixMinCellWidth * columnCount
            val fitCellWidth = ((maxWidth - MatrixHeadWidth) / columnCount).coerceAtLeast(MatrixMinCellWidth)
            val needsScroll = minTotal > maxWidth
            val shape = RoundedCornerShape(DS.rInner)
            val scrollState = rememberScrollState()
            Column(
                Modifier
                    .background(DS.surface, shape)
                    .border(1.dp, DS.line, shape)
                    .clip(shape)
                    .then(if (needsScroll) Modifier.horizontalScroll(scrollState) else Modifier)
            ) {
                MatrixHeaderRow(members, fitCellWidth)
                members.forEachIndexed { i, member ->
                    MatrixRow(member, i, rows.getOrElse(i) { emptyList() }, members, fitCellWidth, selection, onSelectionChange)
                }
            }
        }
        MatrixLegend()
    }
}

@Composable
private fun MatrixHeaderRow(members: List<ImasCallMatrixMember>, cellWidth: Dp) {
    Row(Modifier.padding(vertical = DS.sp3)) {
        Box(Modifier.width(MatrixHeadWidth))
        members.forEach { m ->
            Box(Modifier.width(cellWidth), contentAlignment = Alignment.Center) {
                ImasAvatar(label = m.label, seed = m.seed, size = MatrixAvatarSize, imageUrl = m.imageUrl, reservesPickRing = false)
            }
        }
        Box(Modifier.width(cellWidth), contentAlignment = Alignment.Center) {
            Text(
                "P",
                style = ImasTextRole.BADGE.style.copy(fontWeight = FontWeight.Black),
                color = DS.ink,
                modifier = Modifier.semantics { contentDescription = "プロデューサー" }
            )
        }
    }
    Box(Modifier.fillMaxWidth().height(1.5.dp).background(DS.ink))
}

@Composable
private fun MatrixRow(
    member: ImasCallMatrixMember,
    rowIndex: Int,
    cells: List<ImasCallMatrixCell>,
    members: List<ImasCallMatrixMember>,
    cellWidth: Dp,
    selection: ImasCallMatrixSelection,
    onSelectionChange: (ImasCallMatrixSelection) -> Unit
) {
    Column {
        Row(
            Modifier
                .background(if (rowIndex == selection.row) DS.fill else Color.Transparent)
        ) {
            Box(
                Modifier
                    .width(MatrixHeadWidth)
                    .height(MatrixCellHeight)
                    .imasPress(onClickLabel = "${member.name}の行") {
                        var column = selection.column
                        if (column == rowIndex) column = (rowIndex + 1) % members.size.coerceAtLeast(1)
                        onSelectionChange(ImasCallMatrixSelection(rowIndex, column))
                    },
                contentAlignment = Alignment.Center
            ) {
                ImasAvatar(label = member.label, seed = member.seed, size = MatrixAvatarSize + 4.dp, imageUrl = member.imageUrl, reservesPickRing = false)
            }
            cells.forEachIndexed { j, cell ->
                MatrixCellView(cell, rowIndex, j, member, members, cellWidth, selection, onSelectionChange)
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(DS.sep))
    }
}

@Composable
private fun MatrixCellView(
    cell: ImasCallMatrixCell,
    row: Int,
    column: Int,
    fromMember: ImasCallMatrixMember,
    members: List<ImasCallMatrixMember>,
    cellWidth: Dp,
    selection: ImasCallMatrixSelection,
    onSelectionChange: (ImasCallMatrixSelection) -> Unit
) {
    val isSelected = selection.row == row && selection.column == column
    val selectable = cell.kind == ImasCallMatrixCellKind.MEMBER
    val spoken = when (cell.kind) {
        ImasCallMatrixCellKind.FIRST_PERSON -> "${fromMember.name}の一人称 ${cell.text ?: "未登録"}"
        ImasCallMatrixCellKind.PRODUCER -> "${fromMember.name}からプロデューサーへ ${cell.text ?: "未登録"}"
        ImasCallMatrixCellKind.MEMBER -> "${fromMember.name}から${members.getOrNull(column)?.name ?: ""}へ ${cell.text ?: "未登録"}"
    }
    val background = when {
        cell.kind == ImasCallMatrixCellKind.FIRST_PERSON -> DS.board
        isSelected -> DS.surface
        else -> Color.Transparent
    }
    Box(
        Modifier
            .width(cellWidth)
            .height(MatrixCellHeight)
            .background(background)
            .then(
                if (isSelected) Modifier.border(1.5.dp, DS.ink) else Modifier
            )
            .then(
                if (selectable) {
                    Modifier
                        .imasPress(onClickLabel = spoken) { onSelectionChange(ImasCallMatrixSelection(row, column)) }
                        .semantics { selected = isSelected }
                } else {
                    Modifier.semantics { contentDescription = spoken }
                }
            ),
        contentAlignment = Alignment.Center
    ) {
        Box(Modifier.align(Alignment.CenterStart).fillMaxHeight().width(1.dp).background(DS.sep))
        Text(
            cell.text ?: "—",
            style = ImasType.text(
                11.sp,
                if (cell.kind == ImasCallMatrixCellKind.FIRST_PERSON || isSelected) FontWeight.Black else FontWeight.Medium
            ),
            color = if (cell.kind == ImasCallMatrixCellKind.FIRST_PERSON) DS.boardInk else if (cell.text == null) DS.ink3 else DS.ink,
            textAlign = TextAlign.Center,
            maxLines = 3,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(horizontal = DS.sp1).widthIn(max = cellWidth)
        )
    }
}

@Composable
private fun MatrixLegend() {
    Row(horizontalArrangement = Arrangement.spacedBy(DS.Space.gapLoose)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(DS.sp2)) {
            Box(Modifier.size(12.dp).background(DS.board))
            Text("一人称", style = ImasTextRole.BADGE.style, color = DS.ink2)
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(DS.sp2)) {
            Box(Modifier.size(12.dp).border(1.5.dp, DS.ink))
            Text("選んだ組", style = ImasTextRole.BADGE.style, color = DS.ink2)
        }
    }
}
