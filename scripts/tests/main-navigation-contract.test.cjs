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

test("main screen has Updates and Patch Notes navigation with launcher dock", () => {
  assert.match(app, /Atualizações/);
  assert.match(app, /Patch Notes/);
  assert.match(app, /setActiveSection/);
  assert.match(app, /w-\[56%\].*max-w-\[800px\]/);
  assert.match(app, /absolute right-0 bottom-0/);
  assert.match(
    app,
    /<header[^>]*justify-center[\s\S]*aria-label="Navegação principal"[\s\S]*justify-end/,
  );
  assert.doesNotMatch(app, />\s*Novidades\s*</);
  assert.doesNotMatch(app, /<h1 className="text-xl font-semibold">/);
  assert.match(app, /<LaunchButton/);
});

test("global header aligns Console, Settings, Site, and Discord on the right", () => {
  assert.match(header, /justify-end/);
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
