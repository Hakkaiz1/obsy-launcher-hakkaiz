# Discord Announcements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mirror recent and new messages from two Discord channels into the launcher's Atualizações and Patch Notes tabs through a public JSON feed.

**Architecture:** A scheduled GitHub Actions workflow runs a dependency-free Node.js synchronizer every 10 minutes and commits `public/announcements.json`. The launcher fetches the public feed through a fixed-URL Rust Tauri command and renders the matching channel in the existing tabs.

**Tech Stack:** GitHub Actions, Node.js built-in `fetch` and `node:test`, Rust with existing `reqwest`/`serde`, Tauri 2, React and TypeScript.

**Spec:** `docs/superpowers/specs/2026-10-01-discord-announcements-design.md`

## Global Constraints

- Schedule synchronization every 10 minutes and allow manual workflow dispatch.
- Import up to 100 recent messages per channel initially and retain at most 30 messages per channel in the feed.
- Keep Discord bot credentials only in GitHub Actions secrets; never bundle credentials in the launcher or publish them in logs.
- Fetch and validate both channels before replacing the feed; on failure leave the existing feed untouched and fail the workflow.
- Render message content as text and make only HTTPS attachment URLs actionable.
- The public feed URL is fixed in the Rust backend; accept no feed URL or credential from the frontend.
- Do not run a frontend or Tauri build unless the user explicitly requests one.

## Review Focus

- Missing channel variables or invalid Discord token: fail the workflow without modifying the feed; test both configuration validation and API authorization failure in Task 1.
- Discord rate limits or transient API errors: honor `Retry-After`, surface a sanitized failure, and preserve the previous feed; test in Task 1.
- Out-of-order or repeated messages across pages: deduplicate by ID, advance the cursor to the greatest snowflake, and sort newest first; test in Task 1.
- Invalid or non-HTTPS attachment links: exclude them from actionable links; test feed validation in Tasks 1 and 2.
- Feed unavailable during launcher start or refresh: keep the UI responsive and show a retryable error; cover the loading/error/retry states in Task 3.

---

### Task 1: Synchronize Discord channels into a public feed

**Files:**

- Create: `scripts/sync-discord-announcements.mjs`
- Create: `scripts/tests/discord-announcements-sync.test.cjs`
- Create: `.github/workflows/sync-discord-announcements.yml`
- Create: `public/announcements.json`

**Interfaces:**

- Produces: exported `syncAnnouncements({ env, fetchImpl, feedPath })` returning `Promise<boolean>`; returns `true` when the feed changes and `false` when there are no new messages.
- Produces: feed shape `{ schemaVersion: 1, generatedAt, updates: { cursor, messages }, patchNotes: { cursor, messages } }`. Each message has `{ id, content, author, timestamp, attachments }`.
- Consumes: `DISCORD_BOT_TOKEN` secret and `DISCORD_UPDATES_CHANNEL_ID` / `DISCORD_PATCH_NOTES_CHANNEL_ID` workflow variables.

- [ ] **Step 1: Write failing tests** in `scripts/tests/discord-announcements-sync.test.cjs` for:
  - `importsRecentHistoryAndKeepsThirtyMessages`: expect 30 displayed messages and the greatest ID from the 100-message initial page as the cursor.
  - `paginatesAfterCursorAndDeduplicatesMessages`: expect requests to continue after the saved cursor until an empty page and each ID to appear once in descending snowflake order.
  - `doesNotRewriteFeedWhenThereAreNoNewMessages`: expect `false` and byte-for-byte unchanged feed contents, including `generatedAt`.
  - `preservesExistingFeedOnMissingConfigurationOrDiscordFailure`: expect rejection and byte-for-byte unchanged feed for missing configuration, 401/403, malformed responses, and exhausted 429 retries.
  - `retriesRateLimitsUsingRetryAfter`: expect the next request only after the server-provided delay.
  - `omitsNonHttpsAttachments`: expect the non-HTTPS URL to be absent from the written feed.
- [ ] **Step 2: Run the new test file and verify it fails** because the exported synchronizer does not exist.

  Run: `node --test scripts/tests/discord-announcements-sync.test.cjs`

- [ ] **Step 3: Implement `syncAnnouncements({ env, fetchImpl, feedPath })`** in `scripts/sync-discord-announcements.mjs`. Validate all three configuration values before network calls. Fetch both channel histories using Discord REST API v10 and `Authorization: Bot ...`; use `limit=100`, paginate new messages with `after=<cursor>`, and retry 429 responses using `Retry-After` for at most five attempts. Build the complete next document in memory and write it only after both channels validate; write to a sibling temporary file and rename it over the feed so a failed write cannot truncate the previous feed. Change `generatedAt` only when the displayed feed changes, so idle scheduled runs produce no commit. Use Node's built-in `fs/promises` and `fetch`; do not add packages. Keep messages sorted by numeric Discord snowflake descending, and keep the cursor at the maximum snowflake observed.
- [ ] **Step 4: Implement the workflow** in `.github/workflows/sync-discord-announcements.yml`: schedule every 10 minutes and `workflow_dispatch`; grant only `contents: write`; set `concurrency` to one sync group with cancellation disabled; check out `main`; run the script; commit and push `public/announcements.json` only when it changed. Use only `DISCORD_BOT_TOKEN` as a secret and the two channel IDs as variables. Do not echo environment secrets.
- [ ] **Step 5: Add the valid empty initial feed** at `public/announcements.json` with schema version 1, generation timestamp, empty cursor strings, and empty message arrays for both channels.
- [ ] **Step 6: Run synchronizer tests and verify all pass.**

  Run: `node --test scripts/tests/discord-announcements-sync.test.cjs`

- [ ] **Step 7: Commit the synchronizer, tests, initial feed, and workflow.**

  ```powershell
  git add scripts/sync-discord-announcements.mjs scripts/tests/discord-announcements-sync.test.cjs .github/workflows/sync-discord-announcements.yml public/announcements.json
  git commit -m "feat: sync Discord announcements to public feed"
  ```

### Task 2: Fetch and validate announcements in the Rust backend

**Files:**

- Create: `src-tauri/src/announcements.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**

- Consumes: the feed JSON contract from Task 1.
- Produces: `pub async fn fetch_announcements() -> Result<AnnouncementFeed, String>` exposed as Tauri command `get_discord_announcements`.
- Produces: `pub fn parse_feed(json: &str) -> Result<AnnouncementFeed, String>` for schema and URL validation.
- Produces: serializable `AnnouncementFeed`, `AnnouncementChannel`, and `Announcement` types with the exact JSON field names from Task 1.

- [ ] **Step 1: Create the failing Rust test scaffold** in `src-tauri/src/announcements.rs` for `parse_feed_accepts_valid_feed`, `parse_feed_rejects_unsupported_schema_and_missing_fields`, and `parse_feed_rejects_non_https_attachments`; declare `mod announcements;` in `src-tauri/src/lib.rs` so Cargo compiles the tests.
- [ ] **Step 2: Run the focused Rust tests and verify they fail** with unresolved feed types/validation functions, not because Cargo found zero matching tests.

  Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib announcements::tests`

- [ ] **Step 3: Implement `parse_feed(json: &str) -> Result<AnnouncementFeed, String>`** in `src-tauri/src/announcements.rs`. Use the existing `serde` and `reqwest::Url` types, validate schema version 1, required fields, at most 30 messages per channel, and HTTPS attachment URLs. Implement `fetch_announcements() -> Result<AnnouncementFeed, String>` using one fixed public URL: `https://raw.githubusercontent.com/Hakkaiz1/obsy-launcher-hakkaiz/main/public/announcements.json`. Check HTTP status before parsing and return explicit error messages without fallback content.
- [ ] **Step 4: Register the Tauri command** in `src-tauri/src/lib.rs` by adding `#[tauri::command]` to `fetch_announcements` and adding `announcements::fetch_announcements` to `tauri::generate_handler![]`; keep the module declaration from Step 1.
- [ ] **Step 5: Run focused Rust tests and verify they pass.**

  Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib announcements::tests`

- [ ] **Step 6: Commit the Rust module and command registration.**

  ```powershell
  git add src-tauri/src/announcements.rs src-tauri/src/lib.rs
  git commit -m "feat: expose Discord announcements feed"
  ```

### Task 3: Render the channel feeds in the existing launcher tabs

**Files:**

- Create: `src/components/launcher/AnnouncementsPanel.tsx`
- Modify: `src/App.tsx`
- Modify: `scripts/tests/main-navigation-contract.test.cjs`

**Interfaces:**

- Consumes: Tauri command `get_discord_announcements` and the `AnnouncementFeed` JSON shape from Task 1.
- Produces: `AnnouncementsPanel` accepting `activeSection: "updates" | "patch-notes"` and rendering the corresponding channel's messages.

- [ ] **Step 1: Extend the frontend contract test** to require the active section to map to the correct feed channel and to verify loading, empty, error, retry, message text, author/date, and HTTPS attachment UI. Assert message content is rendered as text and the frontend invokes the fixed Tauri command rather than making a direct Discord request.
- [ ] **Step 2: Run the navigation contract test and verify it fails** because the tabs still show placeholder copy.

  Run: `node --test scripts/tests/main-navigation-contract.test.cjs`

- [ ] **Step 3: Implement `AnnouncementsPanel`** with typed feed state, initial invoke of `get_discord_announcements`, a five-minute refresh interval while mounted, and a manual retry button. Expose explicit loading, empty, and retryable error states. Render content as ordinary React text; render only HTTPS attachment URLs as links with safe external-link behavior.
- [ ] **Step 4: Replace the placeholder paragraph in `src/App.tsx`** with `<AnnouncementsPanel activeSection={activeSection} />`; preserve the existing tab controls and updater status display.
- [ ] **Step 5: Run the frontend contract and all Node contract tests; verify they pass.**

  Run: `node --test scripts/tests/*.test.cjs`

- [ ] **Step 6: Check formatting and editor diagnostics without running a build.**

  Run: `npx prettier --check src/App.tsx src/components/launcher/AnnouncementsPanel.tsx scripts/tests/main-navigation-contract.test.cjs`

  Expected: all files use Prettier code style; no errors reported for the modified TypeScript files.

- [ ] **Step 7: Commit the frontend integration and tests.**

  ```powershell
  git add src/components/launcher/AnnouncementsPanel.tsx src/App.tsx scripts/tests/main-navigation-contract.test.cjs
  git commit -m "feat: show Discord announcements in launcher"
  ```

### Task 4: Verify workflow configuration and live setup

**Files:**

- Verify: `.github/workflows/sync-discord-announcements.yml`
- Verify: `public/announcements.json`

- [ ] **Step 1: Validate the workflow YAML and its permissions** using the repository's available YAML/Actions validation tooling; confirm schedule, manual dispatch, concurrency, and `contents: write` are present and no broader permissions are granted.
- [ ] **Step 2: Review the repository visibility.** The raw feed URL must be publicly readable without authentication; if the launcher repository is private, stop and configure the separate public feed repository and restricted write credential described in the spec before enabling the workflow.
- [ ] **Step 3: Configure repository secrets and variables** (`DISCORD_BOT_TOKEN`, `DISCORD_UPDATES_CHANNEL_ID`, `DISCORD_PATCH_NOTES_CHANNEL_ID`), add the bot to the server with view/history permissions, and enable the Discord Message Content intent.
- [ ] **Step 4: Dispatch the workflow manually** and verify that both channels sync, the feed is publicly readable, and the workflow logs contain no token. Do not claim the live integration is complete until this succeeds.
