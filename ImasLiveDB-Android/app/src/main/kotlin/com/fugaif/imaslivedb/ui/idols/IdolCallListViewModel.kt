package com.fugaif.imaslivedb.ui.idols

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import com.fugaif.imaslivedb.data.model.Idol
import com.fugaif.imaslivedb.di.AppModule
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import uniffi.imas_core.CallDirection
import uniffi.imas_core.IdolCallGroup

data class IdolCallListUiState(
    val idol: Idol? = null,
    val query: String = "",
    val direction: CallDirection = CallDirection.BOTH,
    val groups: List<IdolCallGroup> = emptyList(),
    val others: Map<String, Idol> = emptyMap(),
    val hasLoaded: Boolean = false
)

/**
 * アイドルの呼び方の全員 (iOS `IdolCallListView`)。
 *
 * まとめ方・並び・検索の照合・向きの絞り込み・コピーの文面はコア (`idol_call_groups`)。
 * 打鍵・向き切替ごとに読み込みが走るので、追い越された古い結果で上書きしないよう
 * 前回のジョブを cancel してから次を積む (iOS の `Task.isCancelled` ガードと同じ目的)。
 */
class IdolCallListViewModel(app: Application, private val idolId: String) : AndroidViewModel(app) {

    private val idols = AppModule.from(app).idolRepository

    private val _uiState = MutableStateFlow(IdolCallListUiState())
    val uiState: StateFlow<IdolCallListUiState> = _uiState.asStateFlow()

    private var loadJob: Job? = null

    init {
        viewModelScope.launch {
            val idol = idols.fetchIdol(idolId)
            _uiState.value = _uiState.value.copy(idol = idol)
            reload()
        }
    }

    fun setQuery(query: String) {
        _uiState.value = _uiState.value.copy(query = query)
        reload()
    }

    fun setDirection(direction: CallDirection) {
        _uiState.value = _uiState.value.copy(direction = direction)
        reload()
    }

    private fun reload() {
        // 前回の読み込みを cancel してから積む。打鍵ごとに走るため、追い越された
        // 古い結果が後から others/groups を上書きしないようにする。
        loadJob?.cancel()
        loadJob = viewModelScope.launch {
            val state = _uiState.value
            val loaded = runCatching {
                idols.fetchIdolCallGroups(idolId, state.query, state.direction)
            }.getOrDefault(emptyList())
            val missing = loaded.flatMap { it.pairs.map { pair -> pair.otherId } }.toSet() - _uiState.value.others.keys
            val fetchedOthers = if (missing.isNotEmpty()) {
                runCatching { idols.fetchIdolsByIds(missing.toList()) }.getOrDefault(emptyList())
            } else emptyList()
            _uiState.value = _uiState.value.copy(
                groups = loaded,
                others = _uiState.value.others + fetchedOthers.associateBy { it.id },
                hasLoaded = true
            )
        }
    }

    class Factory(private val app: Application, private val idolId: String) : ViewModelProvider.Factory {
        @Suppress("UNCHECKED_CAST")
        override fun <T : androidx.lifecycle.ViewModel> create(modelClass: Class<T>): T =
            IdolCallListViewModel(app, idolId) as T
    }
}
