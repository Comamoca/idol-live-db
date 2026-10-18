package com.fugaif.imaslivedb.data.community

import android.util.Log
import com.fugaif.imaslivedb.data.auth.AuthService
import com.fugaif.imaslivedb.data.net.WorkerHttpClient
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject

/**
 * 自分の貢献バッジ (`GET /users/me/badges`)。マイ投稿の件数と内訳の出どころ。iOS `BadgeService` の移植。
 * 件数はサーバ (D1) にだけあり、端末には持たない。
 */
class BadgeService(
    private val http: WorkerHttpClient,
    private val authService: AuthService
) {
    /**
     * @property editCount 編集の件数 (アプリからの編集 batch。取り消されたものも数える)。
     * @property categories record_type 別の件数 (batch を数え、セトリの行は ShowSetlist にまとめる)。
     */
    data class Badges(val editCount: Int, val goodsReceived: Int, val categories: Map<String, Int>)

    @Volatile private var cache: Pair<Badges, Long>? = null

    init {
        authService.addSignOutListener { cache = null }
    }

    /** 自分のバッジ。未ログイン・取れなかったときは null。`force` でなければ 5 分以内は手元の値を返す。 */
    suspend fun myBadges(force: Boolean = false): Badges? = withContext(Dispatchers.IO) {
        if (!authService.state.value.isSignedIn) return@withContext null
        val now = System.currentTimeMillis()
        cache?.let { (badges, at) -> if (!force && now - at < CACHE_MS) return@withContext badges }
        try {
            val res = http.request("GET", "/users/me/badges")
            if (!res.isSuccess || res.body.isNullOrEmpty()) {
                Log.w(TAG, "badges -> HTTP ${res.code}")
                return@withContext null
            }
            parse(JSONObject(res.body)).also { cache = it to now }
        } catch (e: Exception) {
            Log.w(TAG, "badges failed: ${e.message}")
            null
        }
    }

    private fun parse(json: JSONObject): Badges {
        val categories = json.optJSONObject("categories")?.let { obj ->
            obj.keys().asSequence().associateWith { obj.optInt(it) }
        }.orEmpty()
        return Badges(
            editCount = json.optInt("editCount"),
            goodsReceived = json.optInt("goodsReceived"),
            categories = categories
        )
    }

    companion object {
        private const val TAG = "BadgeService"
        private const val CACHE_MS = 5 * 60 * 1000L
    }
}
