// 自分の貢献バッジ (GET /users/me/badges)。マイ投稿の件数と内訳の出どころ。

import { describe, expect, it } from "vitest";
import { bearer, callJson } from "./support/worker";
import { exec, insertUser, row } from "./support/d1";

const NOW = Date.now();

/** app の編集 batch を 1 つ入れ、edit_history の行 (record_type) を付ける。 */
async function insertBatch(
  editor: string,
  types: string[],
  opts: { source?: string; cloudkitOk?: number } = {}
): Promise<void> {
  await exec(
    `INSERT INTO edit_batch (editor_id, source, op, summary, cloudkit_ok, created_at)
     VALUES (?, ?, 'replace', '編集', ?, ?)`,
    editor, opts.source ?? "app", opts.cloudkitOk ?? 1, NOW
  );
  const id = (await row<{ id: number }>("SELECT MAX(id) AS id FROM edit_batch"))!.id;
  for (const [i, rt] of types.entries()) {
    await exec(
      `INSERT INTO edit_history (batch_id, record_type, record_name, op, modified_at, created_at)
       VALUES (?, ?, ?, 'update', ?, ?)`,
      id, rt, `${rt}_${id}_${i}`, NOW, NOW
    );
  }
}

describe("GET /users/me/badges", () => {
  it("未ログインは 401", async () => {
    expect((await callJson("GET", "/users/me/badges")).status).toBe(401);
  });

  it("内訳は op 行ではなく編集 batch を数え、セトリの行は ShowSetlist にまとめる", async () => {
    await insertUser("bd.me");
    await insertBatch("bd.me", ["ShowSetlist", "SetlistItem", "SetlistItem", "SetlistItem", "SetlistPerformer"]);
    await insertBatch("bd.me", ["ShowSetlist", "SetlistItem"]);
    await insertBatch("bd.me", ["Song", "SongArtist"]);
    await insertBatch("bd.me", ["Song"], { source: "seed" });
    await insertBatch("bd.me", ["Song"], { cloudkitOk: 0 });

    const res = await callJson("GET", "/users/me/badges", { headers: await bearer("bd.me") });
    expect(res.status).toBe(200);
    expect(res.body.editCount).toBe(3);
    expect(res.body.categories).toEqual({ ShowSetlist: 2, Song: 1, SongArtist: 1 });
  });
});
