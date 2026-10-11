package com.fugaif.imaslivedb.data.model

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey

/**
 * アイドルの髪型 (マスタ)。**みんなで共有する事実**。
 *
 * 1 アイドルに複数の髪型があってよい (版・時期違い)。[isMain] が立っている行が
 * 現在の基準の髪型 (1 アイドルにつき最大 1 件。この不変条件は投入側 `apply_data --check`
 * と共有コアが守る)。[styles] / [accessories] は共有コアの列と同じく '、' 区切りの
 * 1 文字列のまま持つ (配列への分割は読む側)。
 *
 * [origin] は `official` (公式が出典) / `promoted` (ユーザー投稿が採用されたもの)。
 * 判断・並びは一切ここに持たせない。
 *
 * FK は宣言しない (他のマスタ表と揃えてあり、親 (Idol) が後から届く差分同期で子行が
 * 1 件ずつ落ちるのを避けるため)。
 *
 * **共有コアの `master_schema.sql` が持つ一意索引 (`idx_idol_hairstyles_label`
 * (idol_id, label) / `idx_idol_hairstyles_main` (idol_id) WHERE is_main = 1) は
 * 端末の Room 表にはあえて作らない。** 端末は CloudKit の写しで、どの髪型を main に
 * するかの入れ替えが CloudKit 側の複数行更新として届くと、同期の途中で一時的に
 * is_main=1 が 2 行重なる瞬間があり得る。一意索引があるとその瞬間に行が丸ごと
 * 捨てられる (iOS も同じ理由でこの 2 索引は作らず、代わりに upsert のとき
 * is_main=false の行を先に書く順にしている。[com.fugaif.imaslivedb.data.sync.SyncMappers]
 * の `idolHairstyles` 呼び出し側を参照)。
 */
@Entity(
    tableName = "idol_hairstyles",
    indices = [
        Index(name = "idx_idol_hairstyles_idol", value = ["idol_id", "sort_order"])
    ]
)
data class IdolHairstyle(
    @PrimaryKey
    @ColumnInfo(name = "id")
    val id: String,

    @ColumnInfo(name = "idol_id")
    val idolId: String,

    @ColumnInfo(name = "label")
    val label: String,

    @ColumnInfo(name = "is_main", defaultValue = "0")
    val isMain: Boolean = false,

    @ColumnInfo(name = "hair_color")
    val hairColor: String? = null,

    @ColumnInfo(name = "hair_color_secondary")
    val hairColorSecondary: String? = null,

    @ColumnInfo(name = "hair_length")
    val hairLength: String? = null,

    /** '、' 区切りの 1 文字列のまま (共有コアの列と同じ)。 */
    @ColumnInfo(name = "styles")
    val styles: String? = null,

    @ColumnInfo(name = "bangs")
    val bangs: String? = null,

    /** '、' 区切りの 1 文字列のまま (共有コアの列と同じ)。 */
    @ColumnInfo(name = "accessories")
    val accessories: String? = null,

    @ColumnInfo(name = "origin", defaultValue = "'official'")
    val origin: String = "official",

    @ColumnInfo(name = "source_note")
    val sourceNote: String? = null,

    @ColumnInfo(name = "sort_order", defaultValue = "0")
    val sortOrder: Long = 0
)
