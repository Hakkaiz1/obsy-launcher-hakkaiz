import { spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const PORT = 9237;
const USER_DATA_DIR = `/tmp/chrome-obsy-sc-${Date.now()}`;
const SCREENSHOT_DIR = path.resolve(process.cwd(), "docs/images");

if (!fs.existsSync(SCREENSHOT_DIR)) {
  fs.mkdirSync(SCREENSHOT_DIR, { recursive: true });
}

async function sleep(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function startChrome(): Promise<ChildProcess> {
  const chromePath =
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
  const proc = spawn(
    chromePath,
    [
      "--headless=new",
      "--disable-gpu",
      `--user-data-dir=${USER_DATA_DIR}`,
      `--remote-debugging-port=${PORT}`,
      "--window-size=1200,760",
      "--hide-scrollbars",
      "about:blank",
    ],
    { stdio: "ignore" },
  );

  for (let i = 0; i < 40; i++) {
    try {
      const res = await fetch(`http://127.0.0.1:${PORT}/json/list`);
      if (res.ok) {
        const list = (await res.json()) as any[];
        const pageTarget = list.find((t: any) => t.type === "page");
        if (pageTarget) {
          console.log("Chrome target ready on port", PORT);
          return proc;
        }
      }
    } catch {
      // wait
    }
    await sleep(150);
  }
  throw new Error("Failed to connect to Chrome debugging port");
}

class CDPClient {
  private ws: WebSocket;
  private msgId = 0;
  private pending = new Map<
    number,
    { resolve: (val: any) => void; reject: (err: any) => void }
  >();

  constructor(ws: WebSocket) {
    this.ws = ws;
    this.ws.onmessage = (event) => {
      const data = JSON.parse(event.data.toString());
      if (data.id && this.pending.has(data.id)) {
        const { resolve, reject } = this.pending.get(data.id)!;
        this.pending.delete(data.id);
        if (data.error) reject(data.error);
        else resolve(data.result);
      }
    };
  }

  static async connect(wsUrl: string): Promise<CDPClient> {
    const ws = new WebSocket(wsUrl);
    await new Promise((resolve, reject) => {
      ws.onopen = resolve;
      ws.onerror = reject;
    });
    return new CDPClient(ws);
  }

  async send(method: string, params: Record<string, any> = {}): Promise<any> {
    const id = ++this.msgId;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
    });
  }

  async evaluate(expression: string): Promise<any> {
    const res = await this.send("Runtime.evaluate", {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    return res.result?.result?.value;
  }

  async captureScreenshot(filename: string): Promise<void> {
    const res = await this.send("Page.captureScreenshot", {
      format: "png",
    });
    const buffer = Buffer.from(res.data, "base64");
    const filePath = path.join(SCREENSHOT_DIR, filename);
    fs.writeFileSync(filePath, buffer);
    console.log(`✓ Saved screenshot: ${filename} (${buffer.length} bytes)`);
  }
}

async function preparePage(client: CDPClient) {
  await client.send("Page.navigate", { url: "http://127.0.0.1:5173" });
  await sleep(1200);
}

async function main() {
  const chrome = await startChrome();

  try {
    const listRes = await fetch(`http://127.0.0.1:${PORT}/json/list`);
    const pages = (await listRes.json()) as any[];
    const target = pages.find((t: any) => t.type === "page");

    const client = await CDPClient.connect(target.webSocketDebuggerUrl);
    await client.send("Page.enable");
    await client.send("DOM.enable");
    await client.send("Runtime.enable");

    // Add comprehensive Tauri mock
    await client.send("Page.addScriptToEvaluateOnNewDocument", {
      source: `
        localStorage.setItem('hasCompletedOnboarding', 'true');

        window.__TAURI_INTERNALS__ = {
          invoke: async (cmd, args) => {
            if (cmd === 'get_launcher_state') {
              return {
                scale: 1,
                language: "ENGLISH",
                theme: "DARK",
                memoryAmount: 6144,
                autoMemory: false,
                screenWidth: 1920,
                screenHeight: 1080,
                fullscreen: false,
                jvmArguments: "-XX:+UseG1GC -XX:+UnlockExperimentalVMOptions",
                javaPath: "/Library/Java/JavaVirtualMachines/temurin-21.jdk/Contents/Home/bin/java",
                closeAfterLaunch: false,
                selectedProfileId: "prof-1"
              };
            }
            if (cmd === 'get_profiles') {
              return [
                {
                  id: "prof-1",
                  username: "ObsyPlayer",
                  microsoft: true,
                  skinPng: null,
                  slim: false
                },
                {
                  id: "prof-2",
                  username: "SteveDev",
                  microsoft: false,
                  skinPng: null,
                  slim: false
                }
              ];
            }
            if (cmd === 'get_technic_pack_status') {
              return {
                pack_version: "10.8",
                minecraft_version: "1.7.10",
                forge_version: "10.13.4.1558",
                pack_id: 1132904,
                managed_files: []
              };
            }
            if (cmd === 'get_startup_time') return 142;
            if (cmd === 'get_app_memory_usage') return 38;
            if (cmd === 'get_wardrobe') return [];
            return null;
          },
          transformCallback: (callback) => callback,
          transformIPC: (request) => request,
          plugins: {}
        };
      `,
    });

    // 1. Capture Main Dashboard
    console.log("Capturing Dashboard...");
    await preparePage(client);
    await client.captureScreenshot("obsy-dashboard.png");

    // 2. Capture Settings Dialog
    console.log("Capturing Settings...");
    await preparePage(client);
    await client.evaluate(`
      const btn = document.querySelector('header button:has(svg.lucide-settings)');
      if (btn) btn.click();
    `);
    await sleep(800);
    await client.captureScreenshot("obsy-settings.png");

    // 3. Capture Console with clean realistic Minecraft logs
    console.log("Capturing Console with logs...");
    await preparePage(client);
    await client.evaluate(`
      // Clear previous logs and add realistic Minecraft launch logs
      const btn = document.querySelector('header button:has(svg.lucide-terminal)');
      if (btn) btn.click();
    `);
    await sleep(400);

    await client.evaluate(`
      const consoleContainer = document.querySelector('[role="dialog"] .font-mono');
      if (consoleContainer) {
        consoleContainer.innerHTML = \`
          <div class="mb-2"><span class="text-gray-500">[13:37:00]</span> <span class="text-blue-400">[Obsy:Core] Launcher initialized in 142 ms (RAM: 38 MB)</span></div>
          <div class="mb-2"><span class="text-gray-500">[13:37:02]</span> <span class="text-gray-200">[Minecraft] Setting user: ObsyPlayer (UUID: c0ffee00-1337-4242-beef-000000000001)</span></div>
          <div class="mb-2"><span class="text-gray-500">[13:37:03]</span> <span class="text-gray-200">[FabricLoader/GameProvider] Loading 248 mods from instances/1.21.4-fabric/mods</span></div>
          <div class="mb-2"><span class="text-gray-500">[13:37:04]</span> <span class="text-blue-400">[Sodium] Backend: Metal (Apple M-Series) / Shader cache compiled in 310 ms</span></div>
          <div class="mb-2"><span class="text-gray-500">[13:37:05]</span> <span class="text-gray-200">[Minecraft/SoundEngine] OpenAL sound system initialized at 44100Hz</span></div>
          <div class="mb-2"><span class="text-gray-500">[13:37:06]</span> <span class="text-green-400">[Minecraft] Game started successfully! Connected to Obsy Launcher</span></div>
        \`;
      }
    `);
    await sleep(500);
    await client.captureScreenshot("obsy-console.png");

    console.log("All clean screenshots generated successfully!");
  } finally {
    chrome.kill();
    fs.rmSync(USER_DATA_DIR, { recursive: true, force: true });
  }
}

main().catch((err) => {
  console.error("Screenshot error:", err);
  process.exit(1);
});
