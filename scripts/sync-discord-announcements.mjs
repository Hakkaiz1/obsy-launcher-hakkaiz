import { randomUUID } from "node:crypto";
import { readFile, rename, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const DISCORD_API = "https://discord.com/api/v10";
const MAX_MESSAGES = 30;
const PAGE_SIZE = 100;
const MAX_RATE_LIMIT_RETRIES = 5;

const isSnowflake = (value) => typeof value === "string" && /^\d+$/.test(value);

const compareSnowflakes = (left, right) => {
  const leftId = BigInt(left.id);
  const rightId = BigInt(right.id);
  return leftId === rightId ? 0 : leftId > rightId ? -1 : 1;
};

const parseMessage = (message) => {
  if (
    !message ||
    !isSnowflake(message.id) ||
    typeof message.content !== "string" ||
    typeof message.timestamp !== "string" ||
    Number.isNaN(Date.parse(message.timestamp)) ||
    typeof message.author?.username !== "string" ||
    !Array.isArray(message.attachments) ||
    message.attachments.some(
      (attachment) => !attachment || typeof attachment.url !== "string",
    ) ||
    (message.author.global_name != null &&
      typeof message.author.global_name !== "string") ||
    (message.member?.nick != null && typeof message.member.nick !== "string")
  ) {
    throw new Error("Discord returned an invalid announcement message");
  }

  const author = [
    message.member?.nick,
    message.author.global_name,
    message.author.username,
  ].find((name) => typeof name === "string" && name.trim().length > 0);
  if (!author) {
    throw new Error("Discord returned a message without an author name");
  }
  const attachments = message.attachments.flatMap(({ url }) => {
    try {
      const parsed = new URL(url);
      return parsed.protocol === "https:" ? [parsed.href] : [];
    } catch {
      return [];
    }
  });

  return {
    id: message.id,
    content: message.content,
    author,
    timestamp: new Date(message.timestamp).toISOString(),
    attachments,
  };
};

const validateFeed = (feed) => {
  if (
    !feed ||
    feed.schemaVersion !== 1 ||
    typeof feed.generatedAt !== "string" ||
    Number.isNaN(Date.parse(feed.generatedAt))
  ) {
    throw new Error("Existing announcements feed has an unsupported format");
  }

  for (const channelName of ["updates", "patchNotes"]) {
    const channel = feed[channelName];
    if (
      !channel ||
      (channel.cursor !== "" && !isSnowflake(channel.cursor)) ||
      !Array.isArray(channel.messages) ||
      channel.messages.length > MAX_MESSAGES
    ) {
      throw new Error("Existing announcements feed has an unsupported format");
    }
    channel.messages = channel.messages.map((message) => {
      if (
        !message ||
        !isSnowflake(message.id) ||
        typeof message.content !== "string" ||
        typeof message.author !== "string" ||
        typeof message.timestamp !== "string" ||
        Number.isNaN(Date.parse(message.timestamp)) ||
        !Array.isArray(message.attachments) ||
        message.attachments.some((url) => {
          try {
            return new URL(url).protocol !== "https:";
          } catch {
            return true;
          }
        })
      ) {
        throw new Error("Existing announcements feed has an invalid message");
      }
      return message;
    });
  }
  return feed;
};

const readFeed = async (feedPath) => {
  try {
    const contents = await readFile(feedPath, "utf8");
    return validateFeed(JSON.parse(contents));
  } catch (error) {
    if (error.code === "ENOENT") {
      return {
        schemaVersion: 1,
        generatedAt: new Date(0).toISOString(),
        updates: { cursor: "", messages: [] },
        patchNotes: { cursor: "", messages: [] },
      };
    }
    if (error instanceof SyntaxError) {
      throw new Error("Existing announcements feed contains invalid JSON");
    }
    throw error;
  }
};

const wait = (milliseconds) =>
  new Promise((resolve) => setTimeout(resolve, milliseconds));

const requestPage = async ({ channelId, after, token, fetchImpl, sleep }) => {
  const url = new URL(
    `${DISCORD_API}/channels/${encodeURIComponent(channelId)}/messages`,
  );
  url.searchParams.set("limit", String(PAGE_SIZE));
  if (after) url.searchParams.set("after", after);

  for (let retry = 0; ; retry += 1) {
    let response;
    try {
      response = await fetchImpl(url, {
        headers: { Authorization: `Bot ${token}` },
      });
    } catch {
      throw new Error("Could not contact Discord");
    }

    if (response.status === 429) {
      if (retry >= MAX_RATE_LIMIT_RETRIES) {
        throw new Error("Discord rate limit retries were exhausted");
      }
      const retryAfterHeader = response.headers.get("Retry-After");
      let retryAfter =
        retryAfterHeader === null ? Number.NaN : Number(retryAfterHeader);
      if (!Number.isFinite(retryAfter) || retryAfter < 0) {
        try {
          const body = await response.json();
          retryAfter = Number(body.retry_after);
        } catch {
          throw new Error("Discord returned an invalid rate limit response");
        }
      }
      if (!Number.isFinite(retryAfter) || retryAfter < 0) {
        throw new Error("Discord returned an invalid rate limit delay");
      }
      await sleep(retryAfter * 1000);
      continue;
    }

    if (!response.ok) {
      throw new Error(`Discord request failed with HTTP ${response.status}`);
    }

    let body;
    try {
      body = await response.json();
    } catch {
      throw new Error("Discord returned invalid JSON");
    }
    if (!Array.isArray(body)) {
      throw new Error("Discord returned an invalid messages response");
    }
    return body.map(parseMessage);
  }
};

const fetchChannelMessages = async ({
  channelId,
  cursor,
  token,
  fetchImpl,
  sleep,
}) => {
  const messages = [];
  let after = cursor;
  do {
    const page = await requestPage({
      channelId,
      after,
      token,
      fetchImpl,
      sleep,
    });
    messages.push(...page);
    if (!cursor || page.length < PAGE_SIZE) break;

    const nextAfter = page.reduce(
      (latest, message) =>
        BigInt(message.id) > BigInt(latest) ? message.id : latest,
      after,
    );
    if (BigInt(nextAfter) <= BigInt(after)) {
      throw new Error("Discord pagination did not advance");
    }
    after = nextAfter;
  } while (true);

  return messages;
};

const buildChannel = (existing, incoming) => {
  const allMessages = new Map(
    existing.messages.map((message) => [message.id, message]),
  );
  for (const message of incoming) allMessages.set(message.id, message);

  const messages = [...allMessages.values()]
    .sort(compareSnowflakes)
    .slice(0, MAX_MESSAGES);
  const cursor = messages.reduce(
    (latest, message) =>
      !latest || BigInt(message.id) > BigInt(latest) ? message.id : latest,
    existing.cursor,
  );

  return { cursor, messages };
};

const writeFeedAtomically = async (feedPath, feed) => {
  const temporaryPath = `${feedPath}.${randomUUID()}.tmp`;
  await writeFile(temporaryPath, `${JSON.stringify(feed, null, 2)}\n`, "utf8");
  await rename(temporaryPath, feedPath);
};

export const syncAnnouncements = async ({
  env = process.env,
  fetchImpl = fetch,
  feedPath = fileURLToPath(
    new URL("../public/announcements.json", import.meta.url),
  ),
  sleep = wait,
}) => {
  const token = env.DISCORD_BOT_TOKEN?.trim();
  const channelIds = {
    updates: env.DISCORD_UPDATES_CHANNEL_ID?.trim(),
    patchNotes: env.DISCORD_PATCH_NOTES_CHANNEL_ID?.trim(),
  };
  if (!token || !channelIds.updates || !channelIds.patchNotes) {
    throw new Error("Discord bot token and both channel IDs are required");
  }

  const existing = await readFeed(feedPath);
  const incoming = {};
  for (const channelName of ["updates", "patchNotes"]) {
    incoming[channelName] = await fetchChannelMessages({
      channelId: channelIds[channelName],
      cursor: existing[channelName].cursor,
      token,
      fetchImpl,
      sleep,
    });
  }

  const nextFeed = {
    schemaVersion: 1,
    generatedAt: existing.generatedAt,
    updates: buildChannel(existing.updates, incoming.updates),
    patchNotes: buildChannel(existing.patchNotes, incoming.patchNotes),
  };
  const changed =
    JSON.stringify(nextFeed.updates) !== JSON.stringify(existing.updates) ||
    JSON.stringify(nextFeed.patchNotes) !== JSON.stringify(existing.patchNotes);
  if (!changed) return false;

  nextFeed.generatedAt = new Date().toISOString();
  await writeFeedAtomically(feedPath, nextFeed);
  return true;
};

const invokedPath = process.argv[1]
  ? pathToFileURL(path.resolve(process.argv[1])).href
  : "";
if (invokedPath === import.meta.url) {
  syncAnnouncements()
    .then((changed) => {
      if (changed) console.log("Discord announcements feed updated");
      else console.log("Discord announcements feed is already up to date");
    })
    .catch((error) => {
      console.error(`Discord announcements sync failed: ${error.message}`);
      process.exitCode = 1;
    });
}
