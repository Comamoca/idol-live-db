package com.fugaif.imaslivedb.ui.tags

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.runtime.Composable
import com.fugaif.imaslivedb.data.community.CommunityApi
import com.fugaif.imaslivedb.data.model.Vocab
import com.fugaif.imaslivedb.ui.designsystem.ImasConfirmDestructive
import com.fugaif.imaslivedb.ui.designsystem.ImasFilterChip
import com.fugaif.imaslivedb.ui.theme.hexToColor
import uniffi.imas_core.TagCategoryGroup
import uniffi.imas_core.TagCategoryInput
import uniffi.imas_core.VocabularyTerm

// タグ画面群 (TagListScreen / TagDetailScreen / TagCreateSheet / TagEditSheet / TagFilterSheet /
// SongTagPickerSheet ほか) 共通のカテゴリ定義とタグ選択チップ。iOS Views/Tags/*.swift の移植。
// 順位は `ImasRankBadge`、タグ色の丸は `ImasSwatch` を画面側から直接使う (ここに包まない)。

/** タグの作成先プール。曲タグ (tags) / アイドルタグ (idol_tag_master) / ユニットタグ (unit_tag_master) は
 * それぞれ別マスタなので、UI は共通のままこのフラグで作成 API・カテゴリ候補だけ切り替える。iOS TagDomain の移植。 */
enum class TagDomain { SONG, IDOL, UNIT }

/** タグのカテゴリの候補 (先頭は「なし」= 空文字)。語と並びはコアの vocabulary。 */
private fun categoryOptions(terms: List<VocabularyTerm>): List<Pair<String, String>> =
    listOf("" to Vocab.table.tagCategoryNoneLabel) + terms.map { it.value to it.label }

/** 曲タグのカテゴリ候補。 */
val TAG_CATEGORIES: List<Pair<String, String>> get() = categoryOptions(Vocab.table.songTagCategories)

/** アイドルタグのカテゴリ候補。曲タグとは語彙が別。 */
val TAG_CATEGORIES_IDOL: List<Pair<String, String>> get() = categoryOptions(Vocab.table.idolTagCategories)

/** ユニットタグのカテゴリ候補。 */
val TAG_CATEGORIES_UNIT: List<Pair<String, String>> get() = categoryOptions(Vocab.table.unitTagCategories)

fun tagCategoryOptions(domain: TagDomain): List<Pair<String, String>> = when (domain) {
    TagDomain.IDOL -> TAG_CATEGORIES_IDOL
    TagDomain.UNIT -> TAG_CATEGORIES_UNIT
    TagDomain.SONG -> TAG_CATEGORIES
}

fun tagCategoryLabel(category: String?): String =
    TAG_CATEGORIES.firstOrNull { it.first == category }?.second ?: (category ?: "")

fun idolTagCategoryLabel(category: String?): String =
    TAG_CATEGORIES_IDOL.firstOrNull { it.first == category }?.second ?: (category ?: "")

fun unitTagCategoryLabel(category: String?): String =
    TAG_CATEGORIES_UNIT.firstOrNull { it.first == category }?.second ?: (category ?: "")

/** タグの見出し分けの 1 まとまり (見出しと、その中のタグ)。iOS `TagCategorySection`。 */
data class TagCategorySection(
    /** カテゴリの生値。カテゴリの無いタグのまとまりは空文字。 */
    val category: String,
    val label: String,
    val tags: List<CommunityApi.CommunityTag>
)

/**
 * カテゴリごとにまとめる。まとめ方 (並び・「その他」) はコアの [grouper]
 * (`songTagCategoryGroups` 等) が決め、各まとまりの中は今の並びのまま。
 */
fun List<CommunityApi.CommunityTag>.groupedByCategory(
    grouper: (List<TagCategoryInput>) -> List<TagCategoryGroup>
): List<TagCategorySection> {
    val byId = associateBy { it.id }
    return grouper(map { TagCategoryInput(it.id, it.category) }).map { group ->
        TagCategorySection(group.category, group.label, group.tagIds.mapNotNull { byId[it] })
    }
}

/** タグの一覧の並べ方。1 本の一覧 (今の並び順のまま) か、カテゴリごとの見出しに分けるか。iOS `TagListLayout`。 */
enum class TagListLayout(val label: String) { FLAT("一覧"), BY_CATEGORY("カテゴリ別") }

/**
 * タグピッカー (曲・アイドル・ユニット) 共通のタグ選択チップ (iOS `TagSelectChip`)。
 * 配色は `ImasFilterChip(tintColor:)` に委ねる (タグ色を素で塗らず、WCAG のコントラスト計算を通す)。
 *
 * 状態は 3 つ: 未適用 = ニュートラル + ＋ / 選択中 = タグ色の塗り + ✓ (丸) / 適用済み = タグ色の塗り + ✓ (押せない)。
 */
@Composable
fun TagSelectChip(tag: CommunityApi.CommunityTag, isApplied: Boolean, isSelected: Boolean, onClick: () -> Unit) {
    val icon = when {
        isApplied -> Icons.Filled.Check
        isSelected -> Icons.Filled.CheckCircle
        else -> Icons.Filled.Add
    }
    ImasFilterChip(
        label = tag.name,
        // 件数は名前と同じ本文に畳まない (名前が長い時に件数ごと省略されて消えるため)。
        count = if (tag.totalUses > 0) "${tag.totalUses}" else null,
        selected = isApplied || isSelected,
        onClick = onClick,
        tintColor = tag.color?.let { hexToColor(it) },
        icon = icon,
        isDisabled = isApplied
    )
}

/**
 * 自分が付けたタグを外す前の確認。タグのチップはタップで投票トグルなので、
 * 確認なしだと付けた直後の誤タップで簡単に外れてしまう。付ける方向は確認しない。
 * [tag] が null の間は出さない。
 */
@Composable
fun <T> TagRemovalConfirm(tag: T?, name: (T) -> String, onDismiss: () -> Unit, onConfirm: (T) -> Unit) {
    // ImasConfirmDestructive は onDismiss → onConfirm の順に呼ぶ。閉じた後も対象を失わないよう先に掴む。
    val target = tag ?: return
    ImasConfirmDestructive(
        title = "「${name(target)}」を外しますか？",
        isPresented = true,
        onDismiss = onDismiss,
        onConfirm = { onConfirm(target) },
        actionTitle = "外す"
    )
}
