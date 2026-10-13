// discord_inquiries.ts — Discord の窓口フォーラム (問い合わせ・不具合報告) の新しいスレッドを、
// 非公開リポジトリの GitHub issue にする。日次の triage (daily-data-crawl) がそこから拾う。
//
// 5 分 cron から呼ぶ。Discord へは読むだけで、返信・リアクションはしない (オーナーが手で返す)。
// issue の置き場は公開リポジトリにしない — 問い合わせには個人的な話や端末情報が混ざるため、
// INQUIRY_GITHUB_REPO (非公開) が無い間は何もしない。投稿者の名前も issue には載せない
// (スレッドへのリンクから辿れる)。
//
// どこまで issue にしたかは discord_digest_cursors の source = 'inquiries' に、スレッド id
// (snowflake) を 11 ビット右に寄せた値で持つ。snowflake そのものは 2^63 近くて Number に入らないが、
// 寄せた値は 2^53 未満に収まり、同じミリ秒に立った 2 本も (worker/process のビットで) 区別できる。
// 初めての回は「今」を書くだけで、有効にする前から窓口にあったスレッドは issue にしない。
// 1 本ずつ issue を立ててから位置を進めるので、途中で落ちても次の回で続きから。
// 一時的な失敗 (5xx・429・通信) だけ投げて再試行し、そのスレッド固有の失敗 (最初の投稿が消された等) では
// 止まらない — 1 本で詰まると後ろの問い合わせが自動アーカイブで消えていくため。
//
// スクショは Discord の添付 URL が 1 日ほどで失効するので、画像だけリポジトリ (inquiries/ 以下) に
// 写して issue から貼る。base64 にする CPU が cron 全体の上限に効くので、1 回に扱うスレッドと
// 画像の合計を小さく絞る (問い合わせは 5 分に何本も来ない)。

import { bytesToBase64 } from "./bytes";
import type { Env } from "./env";

const DISCORD_API = "https://discord.com/api/v10";
const GITHUB_API = "https://api.github.com";
const CURSOR = "inquiries";
const DISCORD_EPOCH = 1420070400000n;
/** 1 回の cron で issue にするスレッドの上限 (溜まっていた分は次の回へ)。 */
const THREADS_PER_RUN = 2;
/** issue に載せる本文の上限。 */
const CONTENT_MAX = 3000;
/** リポジトリに写す画像の上限 (1 スレッドの枚数・1 回の cron の合計バイト)。 */
const IMAGES_PER_THREAD = 4;
const IMAGE_BYTES_PER_RUN = 2 * 1024 * 1024;

type InquiryEnv = Pick<Env, "DB"> & Partial<Env>;

interface DiscordThread {
  id: string;
  parent_id?: string;
  name: string;
  applied_tags?: string[];
}

interface DiscordAttachment {
  id?: string;
  filename: string;
  url: string;
  size: number;
  content_type?: string;
}

interface DiscordMessage {
  content?: string;
  attachments?: DiscordAttachment[];
}

/** 再試行すれば通りうる失敗か (それ以外はそのスレッド固有の失敗として先へ進む)。 */
function transient(status: number): boolean {
  return status === 429 || status >= 500;
}

/** snowflake の作成時刻 (ミリ秒)。 */
export function snowflakeMillis(id: string): number {
  return Number((BigInt(id) >> 22n) + DISCORD_EPOCH);
}

/** 位置として持つ値 (snowflake を 11 ビット右に寄せたもの。大小は snowflake と同じ)。 */
export function cursorKey(id: string): number {
  return Number(BigInt(id) >> 11n);
}

function cursorKeyAt(ms: number): number {
  return Number(((BigInt(ms) - DISCORD_EPOCH) << 22n) >> 11n);
}

function jst(ms: number): string {
  return new Date(ms + 9 * 3600 * 1000).toISOString().slice(0, 16).replace("T", " ");
}

/** 利用者の文を issue に置くとき、@ (メンバーへのメンション) と #123 (issue 参照) が効かないようにする。 */
function inert(s: string): string {
  return s.replace(/@/g, "@​").replace(/#(\d)/g, "#​$1");
}

/** issue の本文。投稿者の名前は載せない。content が null なら最初の投稿を読めなかった。 */
export function inquiryIssueBody(opts: {
  guildId: string;
  thread: DiscordThread;
  tags: string[];
  content: string | null;
  images: { filename: string; path: string | null }[];
  others: string[];
}): string {
  const { guildId, thread, tags, content, images, others } = opts;
  const text = (content ?? "").trim();
  const clipped = text.length > CONTENT_MAX ? `${text.slice(0, CONTENT_MAX)}…（以下略。スレッドで読む）` : text;
  const quote =
    content === null ? "> (最初の投稿を読めなかった。消されたか Bot に見えない。スレッドで読む)"
    : clipped ? inert(clipped).replace(/^/gm, "> ")
    : "> (本文なし)";
  const lines = [
    `**Discord**: https://discord.com/channels/${guildId}/${thread.id}`,
    `**投稿**: ${jst(snowflakeMillis(thread.id))} (JST)`,
    tags.length ? `**タグ**: ${inert(tags.join(" / "))}` : "",
    "",
    quote,
  ];
  if (images.length) {
    lines.push("", "**画像**:");
    for (const img of images) {
      const alt = img.filename.replace(/[[\]]/g, "_");
      lines.push(img.path ? `![${alt}](${img.path})` : `- ${inert(img.filename)}（写せなかった。スレッドで見る）`);
    }
  }
  if (others.length) lines.push("", `**ほかの添付**: ${inert(others.join(", "))}（スレッドで見る）`);
  lines.push("", "_スレッドに後から付いた返信は issue に入らない。続きはスレッドで読む。_");
  return lines.filter((l, i) => l !== "" || lines[i - 1] !== "").join("\n");
}

export async function syncDiscordInquiries(env: InquiryEnv): Promise<void> {
  const forumId = env.DISCORD_INQUIRY_CHANNEL_ID;
  const repo = env.INQUIRY_GITHUB_REPO;
  if (!env.DISCORD_BOT_TOKEN || !env.DISCORD_GUILD_ID || !forumId || !repo || !env.GITHUB_TOKEN) return;

  const discord = { Authorization: `Bot ${env.DISCORD_BOT_TOKEN}` };
  const github = {
    Authorization: `Bearer ${env.GITHUB_TOKEN}`,
    Accept: "application/vnd.github+json",
    "User-Agent": "imas-live-api",
    "Content-Type": "application/json",
  };

  // 開いているスレッドと、すぐ閉じられた (アーカイブ済みの) スレッドの両方を見る。
  const [active, archived] = await Promise.all([
    fetch(`${DISCORD_API}/guilds/${env.DISCORD_GUILD_ID}/threads/active`, { headers: discord }),
    fetch(`${DISCORD_API}/channels/${forumId}/threads/archived/public?limit=50`, { headers: discord }),
  ]);
  if (!active.ok) throw new Error(`inquiries: active threads ${active.status}`);
  if (!archived.ok) throw new Error(`inquiries: archived threads ${archived.status}`);
  const seen = new Map<string, DiscordThread>();
  for (const res of [active, archived]) {
    const { threads = [] } = (await res.json()) as { threads?: DiscordThread[] };
    for (const t of threads) if (t.parent_id === forumId) seen.set(t.id, t);
  }

  const row = await env.DB.prepare("SELECT last_rowid FROM discord_digest_cursors WHERE source = ?")
    .bind(CURSOR)
    .first<{ last_rowid: number }>();
  if (!row) {
    await saveCursor(env, cursorKeyAt(Date.now()));
    return;
  }
  const after = Number(row.last_rowid);
  const fresh = [...seen.values()]
    .filter((t) => cursorKey(t.id) > after)
    .sort((a, b) => cursorKey(a.id) - cursorKey(b.id))
    .slice(0, THREADS_PER_RUN);
  if (!fresh.length) return;

  // フォーラムのタグ名 (applied_tags は id しか持たない)。読めなくても issue は立てる。
  const tagNames = new Map<string, string>();
  const forum = await fetch(`${DISCORD_API}/channels/${forumId}`, { headers: discord });
  if (forum.ok) {
    const { available_tags = [] } = (await forum.json()) as { available_tags?: { id: string; name: string }[] };
    for (const t of available_tags) tagNames.set(t.id, t.name);
  }

  let imageBudget = IMAGE_BYTES_PER_RUN;
  for (const thread of fresh) {
    // フォーラムのスレッドの最初の投稿は、スレッドと同じ id を持つ。
    const starter = await fetch(`${DISCORD_API}/channels/${thread.id}/messages/${thread.id}`, { headers: discord });
    if (!starter.ok && transient(starter.status)) throw new Error(`inquiries: starter message ${starter.status}`);
    const message: DiscordMessage | null = starter.ok ? ((await starter.json()) as DiscordMessage) : null;

    const images: { filename: string; path: string | null }[] = [];
    const others: string[] = [];
    for (const [i, a] of (message?.attachments ?? []).entries()) {
      const isImage = (a.content_type ?? "").startsWith("image/");
      if (!isImage || images.length >= IMAGES_PER_THREAD || a.size > imageBudget) {
        others.push(a.filename);
        continue;
      }
      imageBudget -= a.size;
      images.push({ filename: a.filename, path: await copyImage(github, repo, thread.id, `${a.id ?? i}`, a) });
    }

    const res = await fetch(`${GITHUB_API}/repos/${repo}/issues`, {
      method: "POST",
      headers: github,
      body: JSON.stringify({
        title: `[Discord] ${thread.name}`.slice(0, 200),
        body: inquiryIssueBody({
          guildId: env.DISCORD_GUILD_ID,
          thread,
          tags: (thread.applied_tags ?? []).map((id) => tagNames.get(id)).filter((n): n is string => !!n),
          content: message ? (message.content ?? "") : null,
          images,
          others,
        }),
        labels: ["discord"],
      }),
    });
    // 権限・リポジトリの設定の誤り (401/403/404/422) は全スレッド共通なので、進めずに止めて気づかせる。
    if (!res.ok) throw new Error(`inquiries: github ${res.status}: ${(await res.text()).slice(0, 200)}`);
    await saveCursor(env, cursorKey(thread.id));
  }
}

async function saveCursor(env: InquiryEnv, key: number): Promise<void> {
  await env.DB.prepare(
    `INSERT INTO discord_digest_cursors (source, last_rowid) VALUES (?, ?)
     ON CONFLICT(source) DO UPDATE SET last_rowid = excluded.last_rowid`
  )
    .bind(CURSOR, key)
    .run();
}

/**
 * 画像を非公開リポジトリに写し、issue に貼るパスを返す。写せなければ null (issue は立てる)。
 * パスは添付ごとに違う (添付 id を頭に付ける)。前の回に写し終えていれば (422) それをそのまま使う。
 */
async function copyImage(
  github: Record<string, string>,
  repo: string,
  threadId: string,
  attachmentKey: string,
  a: DiscordAttachment
): Promise<string | null> {
  const path = `inquiries/${threadId}/${attachmentKey}-${a.filename.replace(/[^\w.-]/g, "_")}`;
  const blob = `https://github.com/${repo}/blob/HEAD/${path}?raw=true`;
  try {
    const img = await fetch(a.url);
    if (!img.ok) return null;
    const put = await fetch(`${GITHUB_API}/repos/${repo}/contents/${path}`, {
      method: "PUT",
      headers: github,
      body: JSON.stringify({
        message: `inquiry ${threadId}: ${a.filename}`,
        content: bytesToBase64(new Uint8Array(await img.arrayBuffer())),
      }),
    });
    // 非公開リポジトリの画像は ?raw=true で issue に表示される (閲覧者の権限で読む)。
    return put.ok || put.status === 422 ? blob : null;
  } catch {
    return null;
  }
}
