package com.fugaif.imaslivedb.data.model

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey

/**
 * アイドルの項目 (マスタ)。**みんなで共有する事実**。
 *
 * 1 項目 1 値につき 1 行。複数値を持つ項目 ([facet]) は同じ idol_id + facet で複数行になる
 * (例: 好きな食べ物を複数挙げている場合)。[origin] は `official` (公式が出典) / `promoted`
 * (ユーザー投稿が採用されたもの) で、表示の区別は呼び出し側が持つ。
 *
 * 判断・並びは一切ここに持たせない。共有コア (`master_schema.sql` の `idol_facets` と
 * 同じ列を Room 側が写しているだけ)。
 *
 * FK は宣言しない (他のマスタ表と揃えてあり、親 (Idol) が後から届く差分同期で子行が
 * 1 件ずつ落ちるのを避けるため)。
 */
@Entity(
    tableName = "idol_facets",
    indices = [Index(name = "idx_idol_facets_idol", value = ["idol_id", "facet"])]
)
data class IdolFacet(
    @PrimaryKey
    @ColumnInfo(name = "id")
    val id: String,

    @ColumnInfo(name = "idol_id")
    val idolId: String,

    @ColumnInfo(name = "facet")
    val facet: String,

    @ColumnInfo(name = "value")
    val value: String,

    @ColumnInfo(name = "origin", defaultValue = "'official'")
    val origin: String = "official",

    @ColumnInfo(name = "source_note")
    val sourceNote: String? = null,

    @ColumnInfo(name = "sort_order", defaultValue = "0")
    val sortOrder: Long = 0
)
