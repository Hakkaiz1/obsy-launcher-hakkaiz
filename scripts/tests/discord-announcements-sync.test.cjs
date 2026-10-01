const assert = require("node:assert/strict");
const fs = require("node:fs/promises");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");

const syncAnnouncementsPromise =
  import("../sync-discord-announcements.mjs").then(
    ({ syncAnnouncements }) => syncAnnouncements,
  );

const env = {
  DISCORD_BOT_TOKEN: "test-token",
  DISCORD_UPDATES_CHANNEL_ID: "updates-channel",
  DISCORD_PATCH_NOTES_CHANNEL_ID: "patch-channel",
};

const message = (id, options = {}) => ({
  id: String(id),
  content: options.content ?? `Message ${id}`,
  author: { username: options.author ?? "tester" },
  timestamp: "2026-10-01T12:00:00.000Z",
  attachments: options.attachments ?? [],
});

const response = (body, status = 200, headers = {}) =>
  new Response(JSON.stringify(body), { status, headers });

const priorFeed = () =>
  JSON.stringify({
    schemaVersion: 1,
    generatedAt: "2026-09-30T12:00:00.000Z",
    updates: { cursor: "100", messages: [] },
    patchNotes: { cursor: "500", messages: [] },
  });

const withFeed = async (initial, run) => {
  const directory = await fs.mkdtemp(path.join(os.tmpdir(), "obsy-feed-"));
  const feedPath = path.join(directory, "announcements.json");
  if (initial !== undefined) {
    await fs.writeFile(feedPath, initial, "utf8");
  }
  try {
    return await run({ directory, feedPath });
  } finally {
    await fs.rm(directory, { recursive: true, force: true });
  }
};

const requests = (fetchImpl) => async (url, options) => {
  const parsed = new URL(url);
  assert.match(
    parsed.pathname,
    /\/api\/v10\/channels\/(updates-channel|patch-channel)\/messages$/,
  );
  assert.equal(options.headers.Authorization, "Bot test-token");
  return fetchImpl(parsed, options);
};

test("imports recent history and keeps the latest thirty messages", async () => {
  const syncAnnouncements = await syncAnnouncementsPromise;
  await withFeed(undefined, async ({ feedPath }) => {
    const seen = [];
    const fetchImpl = async (url) => {
      const parsed = new URL(url);
      seen.push({
        channel: parsed.pathname,
        after: parsed.searchParams.get("after"),
      });
      return response(
        parsed.pathname.includes("updates-channel")
          ? Array.from({ length: 100 }, (_, index) => message(index + 1))
          : [],
      );
    };

    assert.equal(await syncAnnouncements({ env, fetchImpl, feedPath }), true);
    const feed = JSON.parse(await fs.readFile(feedPath, "utf8"));
    assert.equal(feed.schemaVersion, 1);
    assert.equal(feed.updates.cursor, "100");
    assert.equal(feed.updates.messages.length, 30);
    assert.deepEqual(
      feed.updates.messages.slice(0, 2).map(({ id }) => id),
      ["100", "99"],
    );
    assert.equal(feed.patchNotes.cursor, "");
    assert.equal(feed.patchNotes.messages.length, 0);
    assert.equal(seen.length, 2);
    assert.ok(seen.every(({ after }) => after === null));
  });
});

test("paginates after the cursor, deduplicates, sorts, and trims messages", async () => {
  const syncAnnouncements = await syncAnnouncementsPromise;
  const initialFeed = JSON.stringify({
    schemaVersion: 1,
    generatedAt: "2026-09-30T12:00:00.000Z",
    updates: {
      cursor: "100",
      messages: Array.from({ length: 30 }, (_, index) =>
        message(100 - index),
      ).map(({ id, content, author, timestamp }) => ({
        id,
        content,
        author: author.username,
        timestamp,
        attachments: [],
      })),
    },
    patchNotes: { cursor: "500", messages: [] },
  });

  await withFeed(initialFeed, async ({ feedPath }) => {
    const afterValues = [];
    const fetchImpl = async (url) => {
      const parsed = new URL(url);
      if (parsed.pathname.includes("patch-channel")) return response([]);
      const after = parsed.searchParams.get("after");
      afterValues.push(after);
      if (after === "100") {
        return response(
          Array.from({ length: 100 }, (_, index) => message(101 + index)),
        );
      }
      if (after === "200") return response([message(200), message(201)]);
      assert.equal(after, "201");
      return response([]);
    };

    assert.equal(await syncAnnouncements({ env, fetchImpl, feedPath }), true);
    const feed = JSON.parse(await fs.readFile(feedPath, "utf8"));
    assert.deepEqual(afterValues, ["100", "200"]);
    assert.equal(feed.updates.cursor, "201");
    assert.equal(feed.updates.messages.length, 30);
    assert.equal(feed.updates.messages[0].id, "201");
    assert.equal(feed.updates.messages[1].id, "200");
    assert.equal(new Set(feed.updates.messages.map(({ id }) => id)).size, 30);
  });
});

test("does not rewrite the feed when there are no new messages", async () => {
  const syncAnnouncements = await syncAnnouncementsPromise;
  const initialFeed = JSON.stringify({
    schemaVersion: 1,
    generatedAt: "2026-09-30T12:00:00.000Z",
    updates: { cursor: "100", messages: [] },
    patchNotes: { cursor: "500", messages: [] },
  });

  await withFeed(initialFeed, async ({ feedPath }) => {
    const fetchImpl = async () => response([]);
    assert.equal(await syncAnnouncements({ env, fetchImpl, feedPath }), false);
    assert.equal(await fs.readFile(feedPath, "utf8"), initialFeed);
  });
});

test("preserves an existing feed on missing config or Discord failures", async (t) => {
  const syncAnnouncements = await syncAnnouncementsPromise;
  const initialFeed = priorFeed();
  const failures = [
    {
      name: "missing configuration",
      config: { ...env, DISCORD_BOT_TOKEN: "" },
      fetchImpl: async () =>
        assert.fail("must not fetch without configuration"),
    },
    {
      name: "unauthorized bot",
      config: env,
      fetchImpl: async () => response({ message: "Unauthorized" }, 401),
    },
    {
      name: "missing channel permissions",
      config: env,
      fetchImpl: async () => response({ message: "Forbidden" }, 403),
    },
    {
      name: "malformed channel response",
      config: env,
      fetchImpl: async () => response({ messages: [] }),
    },
    {
      name: "malformed message record",
      config: env,
      fetchImpl: async () => response([{ ...message(7), attachments: "bad" }]),
    },
    {
      name: "malformed author display name",
      config: env,
      fetchImpl: async () =>
        response([
          {
            ...message(7),
            author: { username: "tester", global_name: { invalid: true } },
          },
        ]),
    },
    {
      name: "exhausted rate limit retries",
      config: env,
      fetchImpl: async () => response({ retry_after: 0 }, 429),
    },
  ];

  for (const failure of failures) {
    await t.test(failure.name, async () => {
      await withFeed(initialFeed, async ({ feedPath }) => {
        await assert.rejects(
          syncAnnouncements({
            env: failure.config,
            fetchImpl: requests(failure.fetchImpl),
            feedPath,
          }),
        );
        assert.equal(await fs.readFile(feedPath, "utf8"), initialFeed);
      });
    });
  }
});

test("retries rate limits after the server-provided delay", async () => {
  const syncAnnouncements = await syncAnnouncementsPromise;
  await withFeed(undefined, async ({ feedPath }) => {
    let updatesCalls = 0;
    const delays = [];
    const fetchImpl = async (url) => {
      const parsed = new URL(url);
      if (parsed.pathname.includes("patch-channel")) return response([]);
      updatesCalls += 1;
      if (updatesCalls === 1) {
        return response([], 429, { "Retry-After": "0.025" });
      }
      return response([]);
    };

    await syncAnnouncements({
      env,
      fetchImpl,
      feedPath,
      sleep: async (milliseconds) => delays.push(milliseconds),
    });
    assert.equal(updatesCalls, 2);
    assert.deepEqual(delays, [25]);
  });
});

test("uses the retry delay in the Discord response body when the header is absent", async () => {
  const syncAnnouncements = await syncAnnouncementsPromise;
  await withFeed(undefined, async ({ feedPath }) => {
    let updatesCalls = 0;
    const delays = [];
    const fetchImpl = async (url) => {
      const parsed = new URL(url);
      if (parsed.pathname.includes("patch-channel")) return response([]);
      updatesCalls += 1;
      if (updatesCalls === 1) {
        return response({ retry_after: 0.025 }, 429);
      }
      return response([]);
    };

    await syncAnnouncements({
      env,
      fetchImpl,
      feedPath,
      sleep: async (milliseconds) => delays.push(milliseconds),
    });
    assert.equal(updatesCalls, 2);
    assert.deepEqual(delays, [25]);
  });
});

test("preserves the previous feed when the second channel request fails", async () => {
  const syncAnnouncements = await syncAnnouncementsPromise;
  const initialFeed = priorFeed();

  await withFeed(initialFeed, async ({ feedPath }) => {
    const fetchImpl = async (url) => {
      const parsed = new URL(url);
      return parsed.pathname.includes("patch-channel")
        ? response({ message: "Forbidden" }, 403)
        : response([message(101)]);
    };

    await assert.rejects(
      syncAnnouncements({ env, fetchImpl, feedPath }),
      /HTTP 403/,
    );
    assert.equal(await fs.readFile(feedPath, "utf8"), initialFeed);
  });
});

test("omits attachment links that are not HTTPS", async () => {
  const syncAnnouncements = await syncAnnouncementsPromise;
  await withFeed(undefined, async ({ feedPath }) => {
    const fetchImpl = async (url) => {
      const parsed = new URL(url);
      return response(
        parsed.pathname.includes("updates-channel")
          ? [
              message(10, {
                attachments: [
                  { url: "https://cdn.example.com/patch.zip" },
                  { url: "http://example.com/insecure.zip" },
                ],
              }),
            ]
          : [],
      );
    };

    await syncAnnouncements({ env, fetchImpl, feedPath });
    const feed = JSON.parse(await fs.readFile(feedPath, "utf8"));
    assert.deepEqual(feed.updates.messages[0].attachments, [
      "https://cdn.example.com/patch.zip",
    ]);
  });
});
