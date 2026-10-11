package com.fugaif.imaslivedb.ui.tags

import uniffi.imas_core.songTagCategoryGroups
import com.fugaif.imaslivedb.ui.designsystem.ImasSectionHeaderStyle
import com.fugaif.imaslivedb.ui.designsystem.ImasSectionHeader
import com.fugaif.imaslivedb.ui.designsystem.ImasTabs
import com.fugaif.imaslivedb.ui.components.rememberSearchFiltered
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.fugaif.imaslivedb.data.community.CommunityApi
import com.fugaif.imaslivedb.di.AppModule
import com.fugaif.imaslivedb.ui.designsystem.ImasChipRow
import com.fugaif.imaslivedb.ui.designsystem.ImasContentState
import com.fugaif.imaslivedb.ui.designsystem.ImasEmptyState
import com.fugaif.imaslivedb.ui.designsystem.ImasEmptyStateKind
import com.fugaif.imaslivedb.ui.designsystem.ImasListSection
import com.fugaif.imaslivedb.ui.designsystem.ImasRankBadge
import com.fugaif.imaslivedb.ui.designsystem.ImasRemovableChip
import com.fugaif.imaslivedb.ui.designsystem.ImasRowDivider
import com.fugaif.imaslivedb.ui.designsystem.ImasRowLeading
import com.fugaif.imaslivedb.ui.designsystem.ImasRowTrailing
import com.fugaif.imaslivedb.ui.designsystem.ImasSearchField
import com.fugaif.imaslivedb.ui.designsystem.ImasSelectableRow
import com.fugaif.imaslivedb.ui.designsystem.ImasSheetToolbar
import com.fugaif.imaslivedb.ui.designsystem.ImasSheetToolbarKind
import com.fugaif.imaslivedb.ui.designsystem.ImasSkeletonKind
import com.fugaif.imaslivedb.ui.designsystem.ImasStateContainer
import com.fugaif.imaslivedb.ui.designsystem.ImasSwatch
import com.fugaif.imaslivedb.ui.designsystem.ImasSwatchSize
import com.fugaif.imaslivedb.ui.theme.DS

/**
 * 曲一覧をタグで絞り込むための複数選択シート。iOS TagFilterPicker の移植。
 * 複数選択時は AND (全タグを含む曲) で絞り込む想定 (実際の絞り込みは呼び出し側の ViewModel が行う)。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TagFilterSheet(
    initialSelection: List<CommunityApi.CommunityTag>,
    onDismiss: () -> Unit,
    onDone: (List<CommunityApi.CommunityTag>) -> Unit
) {
    val context = LocalContext.current
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)

    var query by remember { mutableStateOf("") }
    var tags by remember { mutableStateOf<List<CommunityApi.CommunityTag>>(emptyList()) }
    var selected by remember { mutableStateOf(initialSelection) }
    var isLoading by remember { mutableStateOf(true) }
    val trimmedQuery = query.trim()
    // 検索していないときの並べ方 (人気順の一覧 / カテゴリ別)。
    var layout by remember { mutableStateOf(TagListLayout.FLAT) }

    // 全件を 1 回だけ取り、検索欄は手元で絞る (照合はコア)。打鍵ごとに取り直すと、古い語の応答が
    // 後から届いて消した後の一覧を上書きしていた (取り消された取得が空の一覧を書くこともあった)。
    LaunchedEffect(Unit) {
        val api = AppModule.from(context).communityApi
        tags = runCatching { api.tags(sort = "popular") }.getOrDefault(emptyList())
        isLoading = false
    }
    val candidates = rememberSearchFiltered(tags, trimmedQuery) { listOf(it.name, it.description) }

    fun toggle(tag: CommunityApi.CommunityTag) {
        selected = if (selected.any { it.id == tag.id }) selected.filterNot { it.id == tag.id } else selected + tag
    }

    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheetState) {
        Column(Modifier.fillMaxWidth().heightIn(min = 320.dp, max = 560.dp)) {
            ImasSheetToolbar(
                kind = ImasSheetToolbarKind.Select(onCancel = onDismiss, onFinish = { onDone(selected); onDismiss() }),
                title = "タグで絞り込み"
            )
            ImasSearchField(
                prompt = "タグ名で検索",
                text = query,
                onTextChange = { query = it },
                modifier = Modifier.padding(horizontal = DS.Space.screen, vertical = DS.Space.gapTight)
            )
            if (selected.isNotEmpty()) {
                ImasListSection(title = "選択中 (${selected.size}) — すべてを含む曲に絞り込み") {
                    ImasChipRow {
                        selected.forEach { tag ->
                            ImasRemovableChip(text = tag.name, onRemove = { toggle(tag) }, seed = tag.color)
                        }
                    }
                }
            }
            if (trimmedQuery.isEmpty()) {
                ImasTabs(
                    labels = TagListLayout.entries.map { it.label },
                    selection = layout.ordinal,
                    onSelect = { layout = TagListLayout.entries[it] },
                    modifier = Modifier.padding(horizontal = DS.Space.screen)
                )
            }
            val state = when {
                isLoading -> ImasContentState.Loading
                candidates.isEmpty() -> ImasContentState.Empty
                else -> ImasContentState.Loaded
            }
            ImasStateContainer(
                state = state,
                modifier = Modifier.weight(1f),
                skeleton = ImasSkeletonKind.List(rows = 6),
                empty = { ImasEmptyState(ImasEmptyStateKind.EMPTY, title = "タグがありません") }
            ) {
                LazyColumn(modifier = Modifier.fillMaxSize()) {
                    if (trimmedQuery.isEmpty() && layout == TagListLayout.BY_CATEGORY) {
                        // カテゴリ (ムード・シーン …) ごとに見出しを分ける。中は人気順。まとめ方はコア。
                        candidates.groupedByCategory(::songTagCategoryGroups).forEach { group ->
                            item(key = "header-${group.category}") {
                                ImasSectionHeader(group.label, count = "${group.tags.size}", style = ImasSectionHeaderStyle.SMALL)
                            }
                            itemsIndexed(group.tags, key = { _, tag -> tag.id }) { idx, tag ->
                                Column(Modifier.fillMaxWidth()) {
                                    TagFilterRow(
                                        tag = tag,
                                        rank = null,
                                        isSelected = selected.any { it.id == tag.id },
                                        onClick = { toggle(tag) }
                                    )
                                    if (idx < group.tags.lastIndex) ImasRowDivider(inset = DS.Space.rowH)
                                }
                            }
                        }
                    } else {
                        itemsIndexed(candidates, key = { _, tag -> tag.id }) { idx, tag ->
                            // 検索していない時は人気順そのものなので順位の札を出す。
                            val rank = if (trimmedQuery.isEmpty()) idx + 1 else null
                            Column(Modifier.fillMaxWidth()) {
                                TagFilterRow(
                                    tag = tag,
                                    rank = rank,
                                    isSelected = selected.any { it.id == tag.id },
                                    onClick = { toggle(tag) }
                                )
                                if (idx < candidates.lastIndex) ImasRowDivider(inset = DS.Space.rowH)
                            }
                        }
                    }
                }
            }
        }
    }
}

/** タグ候補 1 行。[rank] があれば順位の札を出す。 */
@Composable
private fun TagFilterRow(tag: CommunityApi.CommunityTag, rank: Int?, isSelected: Boolean, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(DS.Space.gapTight),
        verticalAlignment = Alignment.CenterVertically
    ) {
        if (rank != null) ImasRankBadge(rank)
        ImasSelectableRow(
            title = tag.name,
            isSelected = isSelected,
            onClick = onClick,
            modifier = Modifier.weight(1f),
            leading = tag.color?.let { hex ->
                ImasRowLeading.Custom(width = ImasSwatchSize.SMALL.diameter) { ImasSwatch(hex, size = ImasSwatchSize.SMALL) }
            } ?: ImasRowLeading.None,
            trailing = if (tag.totalUses > 0) ImasRowTrailing.Value("${tag.totalUses}曲") else ImasRowTrailing.None,
            seed = tag.color
        )
    }
}
