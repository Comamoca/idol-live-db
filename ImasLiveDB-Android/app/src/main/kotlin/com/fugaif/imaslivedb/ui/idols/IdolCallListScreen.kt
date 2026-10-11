package com.fugaif.imaslivedb.ui.idols

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Chat
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Person
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.lifecycle.viewmodel.compose.viewModel
import com.fugaif.imaslivedb.data.model.Idol
import com.fugaif.imaslivedb.ui.designsystem.ImasCallPairHeader
import com.fugaif.imaslivedb.ui.designsystem.ImasCallPairRow
import com.fugaif.imaslivedb.ui.designsystem.ImasCardList
import com.fugaif.imaslivedb.ui.designsystem.ImasEmptyState
import com.fugaif.imaslivedb.ui.designsystem.ImasNote
import com.fugaif.imaslivedb.ui.designsystem.ImasRowDivider
import com.fugaif.imaslivedb.ui.designsystem.ImasSearchField
import com.fugaif.imaslivedb.ui.designsystem.ImasSectionHeader
import com.fugaif.imaslivedb.ui.designsystem.ImasSegmented
import com.fugaif.imaslivedb.ui.theme.DS
import uniffi.imas_core.CallDirection
import uniffi.imas_core.IdolCallPair

/**
 * アイドルの呼び方の全員 (iOS `IdolCallListView`)。1 行に「本人 → 相手」と「相手 → 本人」を並べ、
 * 片方が無ければ「—」。ユニットの仲間を先頭に、ブランドごとにまとめる。
 * 名前・読み・呼び方で探せて、向きで絞れる。行を押すとそのアイドルへ、長押しで呼び方をコピー。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun IdolCallListScreen(
    idolId: String,
    onNavigateBack: () -> Unit,
    onNavigateToIdolDetail: (String) -> Unit,
    viewModel: IdolCallListViewModel = viewModel(
        factory = IdolCallListViewModel.Factory(
            LocalContext.current.applicationContext as android.app.Application, idolId
        )
    )
) {
    val state by viewModel.uiState.collectAsState()
    val idol = state.idol
    val selfLabel = idol?.shortName ?: ""

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("${idol?.name ?: ""}の呼び方", maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = {
                    IconButton(onClick = onNavigateBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "戻る")
                    }
                }
            )
        }
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState())) {
            Column(
                Modifier.fillMaxWidth().padding(top = DS.Space.gap),
                verticalArrangement = Arrangement.spacedBy(DS.Space.gap)
            ) {
                ImasSearchField(
                    prompt = "名前・呼び方で探す",
                    text = state.query,
                    onTextChange = viewModel::setQuery,
                    modifier = Modifier.padding(horizontal = DS.Space.screen)
                )
                ImasSegmented(
                    labels = listOf("両方", "${selfLabel}が呼ぶ", "${selfLabel}を呼ぶ"),
                    selection = directionIndex(state.direction),
                    onSelect = { viewModel.setDirection(directionFor(it)) },
                    modifier = Modifier.fillMaxWidth().padding(horizontal = DS.Space.screen)
                )
                ImasCallPairHeader(
                    outgoingTitle = "$selfLabel → 相手",
                    incomingTitle = "相手 → $selfLabel",
                    modifier = Modifier.padding(horizontal = DS.Space.screen - DS.Space.gapLoose)
                )
            }
            if (state.hasLoaded && state.groups.isEmpty()) {
                ImasEmptyState(
                    icon = Icons.AutoMirrored.Filled.Chat,
                    title = if (state.query.isEmpty()) "呼び方がありません" else "見つかりません",
                    message = if (state.query.isEmpty()) "この向きの呼び方はまだ登録されていません。" else "名前・読み・呼び方を変えてみてください。"
                )
            }
            Column(verticalArrangement = Arrangement.spacedBy(DS.Space.gapLoose)) {
                state.groups.forEach { group ->
                    Column(verticalArrangement = Arrangement.spacedBy(DS.Space.header)) {
                        ImasSectionHeader(
                            group.title, count = "${group.pairs.size}", tight = true,
                            modifier = Modifier.padding(horizontal = DS.Space.screen)
                        )
                        ImasCardList(modifier = Modifier.padding(horizontal = DS.Space.screen)) {
                            group.pairs.forEachIndexed { index, pair ->
                                if (index > 0) ImasRowDivider(inset = DS.Space.gapLoose)
                                CallRow(pair, state.others[pair.otherId], idol, onNavigateToIdolDetail)
                            }
                        }
                    }
                }
            }
            if (state.groups.isNotEmpty()) {
                ImasNote(
                    "呼び方はニコニコ大百科の呼称表・im@sparql (MIT License) などから。場面や時期で変わる呼び方は「／」で並べています。",
                    modifier = Modifier.padding(horizontal = DS.Space.screen, vertical = DS.Space.gap)
                )
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun CallRow(pair: IdolCallPair, other: Idol?, self: Idol?, onNavigateToIdolDetail: (String) -> Unit) {
    val clipboard = LocalClipboardManager.current
    var showMenu by remember { mutableStateOf(false) }
    Column {
        ImasCallPairRow(
            label = other?.shortName ?: pair.otherName,
            name = pair.otherName,
            outgoing = pair.outgoing,
            incoming = pair.incoming,
            selfName = self?.name ?: "",
            seed = other?.color,
            brand = other?.brandId,
            entityId = pair.otherId,
            modifier = Modifier.combinedClickable(
                onClick = { other?.let { onNavigateToIdolDetail(it.id) } },
                onLongClick = { showMenu = true }
            )
        )
        DropdownMenu(expanded = showMenu, onDismissRequest = { showMenu = false }) {
            DropdownMenuItem(
                text = { Text("呼び方をコピー") },
                leadingIcon = { Icon(Icons.Filled.ContentCopy, contentDescription = null) },
                onClick = { clipboard.setText(AnnotatedString(pair.copyText)); showMenu = false }
            )
            if (other != null) {
                DropdownMenuItem(
                    text = { Text("${other.name}を見る") },
                    leadingIcon = { Icon(Icons.Filled.Person, contentDescription = null) },
                    onClick = { showMenu = false; onNavigateToIdolDetail(other.id) }
                )
            }
        }
    }
}

private fun directionIndex(direction: CallDirection): Int = when (direction) {
    CallDirection.BOTH -> 0
    CallDirection.OUTGOING -> 1
    CallDirection.INCOMING -> 2
}

private fun directionFor(index: Int): CallDirection = when (index) {
    1 -> CallDirection.OUTGOING
    2 -> CallDirection.INCOMING
    else -> CallDirection.BOTH
}
