# Discord announcements for the launcher

## Goal

Automatically show messages from two Discord channels in the launcher's
existing **Atualizações** and **Patch Notes** tabs. Import recent history on the
first run and keep adding new messages without duplicates. The launcher must
not contain a Discord bot token or require players to sign in to Discord.

## Selected approach

Use a scheduled GitHub Actions workflow as a short-lived Discord API poller.
This avoids maintaining an always-on bot host. The workflow runs every 10
minutes and can also be started manually. New posts may therefore take several
minutes to appear.

The workflow uses a small Node.js script with the runtime's built-in `fetch`,
so no new package is required. It reads the two configured channels using a
Discord bot token stored in a GitHub Actions secret, then commits the updated
public JSON feed using the workflow's `GITHUB_TOKEN`.

The feed is committed to the launcher repository if it is public. If that
repository is private, the feed must instead be committed to a public
repository, and the launcher must use that repository's raw-content URL. In
that case, configure a separate fine-grained GitHub token or GitHub App
credential, limited to contents write access for the public feed repository.
Players need unauthenticated read access to the feed.

## Synchronization

- Configure `DISCORD_BOT_TOKEN` as a repository Actions secret.
- Configure `DISCORD_UPDATES_CHANNEL_ID` and
  `DISCORD_PATCH_NOTES_CHANNEL_ID` as repository Actions variables.
- The bot must be a member of the Discord server and have permission to view
  both configured channels and read their message history. Enable Discord's
  Message Content intent for the bot.
- On first run, import up to 100 recent messages per channel, retaining the
  latest 30 for display. Persist each channel's last synchronized Discord
  message ID as a cursor.
- On later runs, request messages after each cursor, following Discord
  pagination until all new messages are fetched. Deduplicate by Discord message
  ID and retain only the latest 30 per channel in the display feed.
- Display messages newest first. Include message ID, text, author, timestamp,
  and attachment URLs. Do not mirror edits or deletions in this initial version.
- Fetch and validate both channels before replacing the feed. If either
  request fails, leave the previous feed untouched and fail the workflow.
  Respect Discord rate-limit retry instructions; never print the bot token.
- Restrict workflow permissions to `contents: write` and prevent overlapping
  synchronization runs.
- If a separate feed repository is used, grant its credential access only to
  that repository and only the permissions needed to update the feed.

## Feed contract

Store a versioned JSON document with a generation timestamp and separate
`updates` and `patchNotes` channel records. Each record contains its last
message ID and an array of at most 30 messages. A message contains:

- `id`: Discord message ID, as a string
- `content`: plain message text
- `author`: display name
- `timestamp`: ISO 8601 timestamp
- `attachments`: array of HTTPS URLs

The launcher validates this shape before presenting it. Message text is
rendered as text, not executable HTML. Only HTTPS attachment links are
actionable.

## Launcher integration

Add a Rust module that fetches the public feed over HTTPS with the existing
`reqwest` dependency and expose it through a Tauri command. The command uses a
fixed public feed URL; it accepts no URL or credential from the frontend.

Replace the placeholder content in the two existing tabs with the matching
feed records. Show loading, empty, and error states, with a retry action for
errors. Load on startup, refresh while the launcher remains open, and allow
manual refresh. The launcher must never access Discord directly.

## Failure handling and security

- Missing configuration, invalid credentials, missing channel permissions,
  unexpected API responses, pagination errors, and feed validation failures
  must be visible in workflow logs without secrets.
- A failed synchronization does not overwrite a valid feed.
- If the launcher cannot reach the feed, keep the UI usable and show an
  explicit retryable error instead of fabricated success or silently empty
  content.
- The Discord token exists only in GitHub Actions secrets. The public feed
  contains only announcement data intended for players.
- React renders message content as text. Attachment links are accepted only
  when they use HTTPS.

## Testing

- Node tests cover initial import, cursor-based pagination, deduplication,
  retention limits, response validation, rate-limit/error behavior, and
  preserving the previous feed when either channel fails.
- Rust tests cover feed deserialization and rejection of invalid feed data.
- Frontend contract tests cover mapping each feed channel to its correct tab
  and rendering loading, empty, error, retry, message, and attachment states.
- Validate workflow syntax and permissions. Exercise a manual workflow run
  with configured Discord credentials before enabling the scheduled run.

## Out of scope

- A custom web dashboard or Discord slash commands.
- Real-time Gateway events or an always-on bot host.
- Editing or deleting mirrored posts from Discord.
- Rich Discord embeds, reactions, or interactive launcher publishing.
- Adding Discord authentication or bot credentials to the launcher.
