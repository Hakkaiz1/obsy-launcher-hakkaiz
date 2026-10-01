const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const app = fs.readFileSync(
  path.join(__dirname, "..", "..", "src", "App.tsx"),
  "utf8",
);
const header = fs.readFileSync(
  path.join(
    __dirname,
    "..",
    "..",
    "src",
    "components",
    "launcher",
    "Header.tsx",
  ),
  "utf8",
);
const globalCss = fs.readFileSync(
  path.join(__dirname, "..", "..", "src", "index.css"),
  "utf8",
);
const announcementsPanelPath = path.join(
  __dirname,
  "..",
  "..",
  "src",
  "components",
  "launcher",
  "AnnouncementsPanel.tsx",
);
const announcementsPanel = fs.existsSync(announcementsPanelPath)
  ? fs.readFileSync(announcementsPanelPath, "utf8")
  : "";

test("main screen has Updates and Patch Notes navigation with launcher dock", () => {
  assert.match(app, /Atualizações/);
  assert.match(app, /Patch Notes/);
  assert.match(app, /setActiveSection/);
  assert.match(app, /w-\[56%\].*max-w-\[800px\]/);
  assert.match(app, /absolute right-10 bottom-10/);
  assert.match(
    app,
    /<header[^>]*justify-center[\s\S]*aria-label="Navegação principal"[\s\S]*justify-end/,
  );
  assert.match(app, /<main className="absolute inset-0 z-10 flex gap-4 p-2">/);
  assert.match(app, /<Header \/>/);
  assert.doesNotMatch(app, />\s*Novidades\s*</);
  assert.doesNotMatch(app, /<h1 className="text-xl font-semibold">/);
  assert.match(app, /<LaunchButton/);
});

test("active announcement tab buttons use the launcher gradient", () => {
  assert.equal(
    [...app.matchAll(/from-primary to-primary\/70 text-primary-foreground/g)]
      .length,
    2,
  );
  assert.doesNotMatch(app, /"bg-primary text-primary-foreground"/);
});

test("global header aligns Console, Settings, Site, and Discord on the right", () => {
  assert.match(header, /justify-end/);
  assert.match(header, /data-tauri-drag-region/);
  assert.doesNotMatch(header, /<header[^>]*backdrop-blur/);
  assert.doesNotMatch(header, /<header[^>]*border-b/);
  assert.match(header, /<header[^>]*absolute top-0 right-0 z-20/);
  assert.doesNotMatch(header, /<header[^>]*w-full/);
  assert.match(header, /<div[^>]*bg-card\/50[^>]*backdrop-blur-md/);
  assert.match(header, /<ConsoleDialog/);
  assert.match(header, /<SettingsDialog/);
  assert.match(header, /Site/);
  assert.match(header, /Discord/);
  assert.match(header, /https:\/\/dbcsuper\.lojasquare\.com\.br\//);
  assert.match(header, /https:\/\/discord\.gg\/p3fTQ3KFbV/);
  assert.match(header, /openUrl/);
  assert.doesNotMatch(app, /https:\/\/dbcsuper\.lojasquare\.com\.br\//);
  assert.doesNotMatch(app, /https:\/\/discord\.gg\/p3fTQ3KFbV/);
});
test("announcement tabs load and render their corresponding public feed channel", () => {
  assert.match(app, /<AnnouncementsPanel activeSection=\{activeSection\} \/>/);
  assert.match(
    announcementsPanel,
    /activeSection === "updates" \? feed\?\.updates : feed\?\.patchNotes/,
  );
  assert.match(announcementsPanel, /invoke<AnnouncementFeed>/);
  assert.match(announcementsPanel, /"fetch_announcements"/);
  assert.match(announcementsPanel, /setInterval/);
  assert.match(announcementsPanel, /REFRESH_INTERVAL_MS/);
  assert.match(announcementsPanel, /Carregando/);
  assert.match(announcementsPanel, /Ainda não há publicações/);
  assert.match(announcementsPanel, /role="alert"/);
  assert.match(announcementsPanel, /Tentar novamente/);
  assert.match(announcementsPanel, /\{message\.content\}/);
  assert.match(announcementsPanel, /\{message\.author\}/);
  assert.match(announcementsPanel, /message\.timestamp/);
  assert.match(announcementsPanel, /message\.attachments\.map/);
  assert.doesNotMatch(announcementsPanel, /dangerouslySetInnerHTML/);
  assert.doesNotMatch(announcementsPanel, /discord\.com\/api/);
});

test("global scrollbars use the launcher's thin purple theme", () => {
  assert.match(
    globalCss,
    /\*\s*\{[^}]*scrollbar-color:\s*rgb\(147 51 234 \/ 55%\)\s+transparent/,
  );
  assert.match(globalCss, /\*\s*\{[^}]*scrollbar-width:\s*thin/);
  assert.match(globalCss, /\*::-webkit-scrollbar-thumb:hover/);
});
