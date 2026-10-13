// Discord の窓口フォーラム → 非公開リポジトリの issue (discord_inquiries.ts)。
// Discord / GitHub への通信は fetchMock で返す。
import { fetchMock } from "cloudflare:test";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { makeEnv } from "./support/worker";
import { exec, row } from "./support/d1";
import { cursorKey, inquiryIssueBody, snowflakeMillis, syncDiscordInquiries } from "../src/discord_inquiries";

const DISCORD = "https://discord.com";
const GITHUB_API = "https://api.github.com";
const CDN = "https://cdn.discordapp.com";
const GUILD = "1530435001231868076";
const FORUM = "1560000000000000000";
const REPO = "fuga-if/inbox";

/** 指定時刻に作られた snowflake (下位ビットで同じミリ秒の別スレッドを作れる)。 */
const flake = (iso: string, seq = 0) => (((BigInt(Date.parse(iso)) - 1420070400000n) << 22n) + BigInt(seq)).toString();

const env = () =>
  makeEnv({ DISCORD_BOT_TOKEN: "bot-token", DISCORD_INQUIRY_CHANNEL_ID: FORUM, INQUIRY_GITHUB_REPO: REPO, GITHUB_TOKEN: "gh-token" });

const cursor = async () =>
  (await row<{ last_rowid: number }>("SELECT last_rowid FROM discord_digest_cursors WHERE source = 'inquiries'"))?.last_rowid ?? null;

/** 位置を「この時刻まで処理済み」にしておく。 */
const startAt = (iso: string) =>
  exec(
    `INSERT INTO discord_digest_cursors (source, last_rowid) VALUES ('inquiries', ?)
     ON CONFLICT(source) DO UPDATE SET last_rowid = excluded.last_rowid`,
    cursorKey(flake(iso))
  );

beforeEach(async () => {
  await exec("DELETE FROM discord_digest_cursors WHERE source = 'inquiries'");
});

afterEach(() => {
  fetchMock.assertNoPendingInterceptors();
});

function listThreads(active: unknown[], archived: unknown[] = []) {
  fetchMock.get(DISCORD).intercept({ path: `/api/v10/guilds/${GUILD}/threads/active`, method: "GET" }).reply(200, { threads: active });
  fetchMock.get(DISCORD).intercept({ path: `/api/v10/channels/${FORUM}/threads/archived/public?limit=50`, method: "GET" })
    .reply(200, { threads: archived });
}

function forumTags(tags: { id: string; name: string }[] = []) {
  fetchMock.get(DISCORD).intercept({ path: `/api/v10/channels/${FORUM}`, method: "GET" }).reply(200, { available_tags: tags });
}

function starter(id: string, status: number, body: unknown = {}) {
  fetchMock.get(DISCORD).intercept({ path: `/api/v10/channels/${id}/messages/${id}`, method: "GET" }).reply(status, body as any);
}

function issues(n: number): any[] {
  const got: any[] = [];
  for (let i = 0; i < n; i++) {
    fetchMock.get(GITHUB_API).intercept({ path: `/repos/${REPO}/issues`, method: "POST" })
      .reply(201, (opts) => {
        got.push(JSON.parse(String(opts.body)));
        return { number: got.length };
      });
  }
  return got;
}

describe("Discord の問い合わせを issue にする (5 分 cron)", () => {
  it("snowflake の時刻を読み、位置は同じミリ秒の 2 本を区別する", () => {
    expect(snowflakeMillis(flake("2026-10-13T01:00:00Z"))).toBe(Date.parse("2026-10-13T01:00:00Z"));
    expect(cursorKey(flake("2026-10-13T01:00:00Z", 1 << 12))).toBeGreaterThan(cursorKey(flake("2026-10-13T01:00:00Z")));
    expect(Number.isSafeInteger(cursorKey(flake("2030-01-01T00:00:00Z")))).toBe(true);
  });

  it("初めての回は位置を今にするだけで、前からあったスレッドは issue にしない", async () => {
    listThreads([{ id: flake("2026-01-01T00:00:00Z"), parent_id: FORUM, name: "古い" }]);
    await syncDiscordInquiries(env());
    expect(await cursor()).toBeGreaterThan(cursorKey(flake("2026-01-01T00:00:00Z")));
  });

  it("窓口の新しいスレッド (閉じられたものも) を古い順に issue にし、次の回は二重に立てない", async () => {
    await startAt("2026-10-13T00:00:00Z");
    const t1 = flake("2026-10-13T01:00:00Z");
    const t2 = flake("2026-10-13T02:00:00Z");
    const active = [
      { id: t2, parent_id: FORUM, name: "歌詞が表示されない", applied_tags: ["tag_bug"] },
      { id: flake("2026-10-13T03:00:00Z"), parent_id: "other-channel", name: "ライブ実況" },
    ];
    const archived = [{ id: t1, parent_id: FORUM, name: "曲の日付が違う" }];
    listThreads(active, archived);
    forumTags([{ id: "tag_bug", name: "不具合" }]);
    starter(t1, 200, { content: "発売日が 1 日ずれています @everyone #12", attachments: [] });
    starter(t2, 200, {
      content: "開くと真っ白です",
      attachments: [
        { id: "a1", filename: "shot [1].png", url: `${CDN}/a/shot1.png`, size: 10, content_type: "image/png" },
        { id: "a2", filename: "shot [1].png", url: `${CDN}/a/shot2.png`, size: 10, content_type: "image/png" },
        { id: "a3", filename: "log.txt", url: `${CDN}/a/log.txt`, size: 10, content_type: "text/plain" },
      ],
    });
    fetchMock.get(CDN).intercept({ path: "/a/shot1.png", method: "GET" }).reply(200, "PNGDATA");
    fetchMock.get(CDN).intercept({ path: "/a/shot2.png", method: "GET" }).reply(200, "PNG2");
    const puts: string[] = [];
    let uploaded: any = null;
    fetchMock.get(GITHUB_API).intercept({ path: `/repos/${REPO}/contents/inquiries/${t2}/a1-shot__1_.png`, method: "PUT" })
      .reply(201, (opts) => {
        uploaded = JSON.parse(String(opts.body));
        puts.push("a1");
        return {};
      });
    // 前の回に写し終えていた (422) ものも、そのパスを使う。
    fetchMock.get(GITHUB_API).intercept({ path: `/repos/${REPO}/contents/inquiries/${t2}/a2-shot__1_.png`, method: "PUT" })
      .reply(422, () => {
        puts.push("a2");
        return { message: "sha wasn't supplied" };
      });
    const got = issues(2);

    await syncDiscordInquiries(env());

    expect(got.map((i) => i.title)).toEqual(["[Discord] 曲の日付が違う", "[Discord] 歌詞が表示されない"]);
    expect(got[0].labels).toEqual(["discord"]);
    expect(got[0].body).toContain(`https://discord.com/channels/${GUILD}/${t1}`);
    expect(got[0].body).toContain("> 発売日が 1 日ずれています @​everyone #​12");
    expect(got[1].body).toContain("**タグ**: 不具合");
    expect(got[1].body).toContain(`![shot _1_.png](https://github.com/${REPO}/blob/HEAD/inquiries/${t2}/a1-shot__1_.png?raw=true)`);
    expect(got[1].body).toContain(`![shot _1_.png](https://github.com/${REPO}/blob/HEAD/inquiries/${t2}/a2-shot__1_.png?raw=true)`);
    expect(got[1].body).toContain("**ほかの添付**: log.txt");
    expect(puts).toEqual(["a1", "a2"]);
    expect(atob(uploaded.content)).toBe("PNGDATA");
    expect(await cursor()).toBe(cursorKey(t2));

    // 次の回: 同じスレッドしか無ければ何も立てない (interceptor が無いので、立てれば落ちる)。
    listThreads(active, archived);
    await syncDiscordInquiries(env());
  });

  it("1 回に立てる数を超えた分は次の回に持ち越す", async () => {
    await startAt("2026-10-13T00:00:00Z");
    const ids = ["01", "02", "03"].map((h) => flake(`2026-10-13T${h}:00:00Z`));
    const threads = ids.map((id, i) => ({ id, parent_id: FORUM, name: `q${i}` }));
    listThreads(threads);
    forumTags();
    starter(ids[0], 200, { content: "a" });
    starter(ids[1], 200, { content: "b" });
    const first = issues(2);
    await syncDiscordInquiries(env());
    expect(first.map((i) => i.title)).toEqual(["[Discord] q0", "[Discord] q1"]);

    listThreads(threads);
    forumTags();
    starter(ids[2], 200, { content: "c" });
    const second = issues(1);
    await syncDiscordInquiries(env());
    expect(second.map((i) => i.title)).toEqual(["[Discord] q2"]);
  });

  it("最初の投稿が読めないスレッドでも issue にして先へ進む (後ろの問い合わせを止めない)", async () => {
    await startAt("2026-10-13T00:00:00Z");
    const gone = flake("2026-10-13T01:00:00Z");
    const next = flake("2026-10-13T02:00:00Z");
    listThreads([{ id: gone, parent_id: FORUM, name: "消された" }, { id: next, parent_id: FORUM, name: "次" }]);
    forumTags();
    starter(gone, 404, { message: "Unknown Message" });
    starter(next, 200, { content: "ok" });
    const got = issues(2);
    await syncDiscordInquiries(env());
    expect(got[0].body).toContain("最初の投稿を読めなかった");
    expect(await cursor()).toBe(cursorKey(next));
  });

  it("一時的な失敗 (Discord の 5xx・GitHub の失敗) では位置を進めず、次の回にやり直す", async () => {
    await startAt("2026-10-13T00:00:00Z");
    const before = await cursor();
    const t1 = flake("2026-10-13T05:00:00Z");
    listThreads([{ id: t1, parent_id: FORUM, name: "質問" }]);
    forumTags();
    starter(t1, 503);
    await expect(syncDiscordInquiries(env())).rejects.toThrow(/starter message 503/);

    listThreads([{ id: t1, parent_id: FORUM, name: "質問" }]);
    forumTags();
    starter(t1, 200, { content: "?" });
    fetchMock.get(GITHUB_API).intercept({ path: `/repos/${REPO}/issues`, method: "POST" }).reply(404, "Not Found");
    await expect(syncDiscordInquiries(env())).rejects.toThrow(/github 404/);
    expect(await cursor()).toBe(before);
  });

  it("スレッドの一覧が取れなければ投げる", async () => {
    fetchMock.get(DISCORD).intercept({ path: `/api/v10/guilds/${GUILD}/threads/active`, method: "GET" }).reply(500, {});
    fetchMock.get(DISCORD).intercept({ path: `/api/v10/channels/${FORUM}/threads/archived/public?limit=50`, method: "GET" })
      .reply(200, { threads: [] });
    await expect(syncDiscordInquiries(env())).rejects.toThrow(/active threads 500/);
  });

  it("issue の置き場 (非公開リポジトリ) が無ければ Discord も読まない", async () => {
    await syncDiscordInquiries(makeEnv({ DISCORD_BOT_TOKEN: "bot-token", DISCORD_INQUIRY_CHANNEL_ID: FORUM, GITHUB_TOKEN: "gh-token" }));
  });

  it("長い本文は切り、投稿者の名前は載せない", () => {
    const body = inquiryIssueBody({
      guildId: GUILD,
      thread: { id: flake("2026-10-13T01:00:00Z"), name: "x" },
      tags: [],
      content: "あ".repeat(3500),
      images: [],
      others: [],
    });
    expect(body).toContain("…（以下略。スレッドで読む）");
    expect(body).toContain("**投稿**: 2026-10-13 10:00 (JST)");
    expect(body).not.toContain("**タグ**");
    expect(body).not.toMatch(/\n\n\n/);
  });
});
