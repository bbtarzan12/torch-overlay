import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";
import net from "node:net";

const root = process.cwd();
const artifactsDir = join(root, "artifacts", "smoke");
const screenshotPath = join(artifactsDir, "web-smoke.png");
const isWindows = process.platform === "win32";

async function main() {
await mkdir(artifactsDir, { recursive: true });

const previewPort = await getFreePort();
const debugPort = await getFreePort();
const profileDir = await mkdtemp(join(tmpdir(), "torch-overlay-smoke-"));
let previewProcess;
let browserProcess;

try {
  await runNpm(["run", "build"]);

  previewProcess = spawn(...npmSpawnArgs(["run", "preview", "--", "--port", String(previewPort), "--strictPort"]), {
    cwd: root,
    stdio: ["ignore", "pipe", "pipe"]
  });

  previewProcess.stdout.on("data", (data) => process.stdout.write(data));
  previewProcess.stderr.on("data", (data) => process.stderr.write(data));

  const appUrl = `http://127.0.0.1:${previewPort}`;
  await waitForHttp(appUrl, 20_000);

  const browser = findBrowser();
  browserProcess = spawn(
    browser,
    [
      "--headless=new",
      "--disable-gpu",
      "--no-first-run",
      "--no-default-browser-check",
      `--remote-debugging-port=${debugPort}`,
      `--user-data-dir=${profileDir}`,
      "--window-size=1500,665",
      "about:blank"
    ],
    { stdio: ["ignore", "pipe", "pipe"] }
  );

  const page = await openDebugPage(debugPort, appUrl);
  const errors = [];
  page.onRuntimeException((message) => errors.push(message));
  await page.navigate(appUrl);
  await page.wait(700);

  const result = await page.evaluate(`(() => {
    const bar = document.querySelector(".tracker-bar");
    const fatal = document.querySelector(".fatal-error");
    const rect = bar?.getBoundingClientRect();

    return {
      title: document.title,
      bodyText: document.body.innerText,
      hasBar: Boolean(bar),
      hasFatalError: Boolean(fatal),
      fatalText: fatal?.textContent ?? "",
      barRect: rect ? {
        x: Math.round(rect.x),
        y: Math.round(rect.y),
        width: Math.round(rect.width),
        height: Math.round(rect.height)
      } : null
    };
  })()`);

  const screenshot = await page.captureScreenshot();
  await writeFile(screenshotPath, Buffer.from(screenshot, "base64"));
  page.close();

  const failures = [];

  if (!result.hasBar) {
    failures.push("tracker bar was not rendered");
  }

  if (result.hasFatalError) {
    failures.push(`fatal error overlay rendered: ${result.fatalText}`);
  }

  if (!result.bodyText.includes("결정")) {
    failures.push("expected Korean crystal text was not rendered");
  }

  if (!result.barRect || result.barRect.width < 1200 || result.barRect.height < 24) {
    failures.push(`unexpected bar rect: ${JSON.stringify(result.barRect)}`);
  }

  if (errors.length > 0) {
    failures.push(`runtime exceptions: ${errors.join(" | ")}`);
  }

  if (failures.length > 0) {
    throw new Error(`${failures.join("\n")}\nScreenshot: ${screenshotPath}`);
  }

  console.log(
    JSON.stringify(
      {
        ok: true,
        url: appUrl,
        screenshot: screenshotPath,
        title: result.title,
        barRect: result.barRect
      },
      null,
      2
    )
  );
} finally {
  await terminateProcess(browserProcess);
  await terminateProcess(previewProcess);
  await rm(profileDir, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
}
}

function findBrowser() {
  const candidates =
    process.platform === "win32"
      ? [
          join(process.env.PROGRAMFILES ?? "", "Google", "Chrome", "Application", "chrome.exe"),
          join(process.env["PROGRAMFILES(X86)"] ?? "", "Google", "Chrome", "Application", "chrome.exe"),
          join(process.env.LOCALAPPDATA ?? "", "Google", "Chrome", "Application", "chrome.exe"),
          join(process.env.PROGRAMFILES ?? "", "Microsoft", "Edge", "Application", "msedge.exe"),
          join(process.env["PROGRAMFILES(X86)"] ?? "", "Microsoft", "Edge", "Application", "msedge.exe")
        ]
      : ["google-chrome", "chromium", "chromium-browser", "microsoft-edge"];

  const browser = candidates.find((candidate) => existsSync(candidate));

  if (!browser) {
    throw new Error("Chrome or Edge executable was not found.");
  }

  return browser;
}

async function openDebugPage(port, url) {
  await waitForHttp(`http://127.0.0.1:${port}/json/version`, 20_000);
  const response = await fetch(`http://127.0.0.1:${port}/json/new?${encodeURIComponent(url)}`, {
    method: "PUT"
  });
  const target = await response.json();
  const client = await CdpClient.connect(target.webSocketDebuggerUrl);

  await client.send("Page.enable");
  await client.send("Runtime.enable");

  return {
    close: () => client.close(),
    wait: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
    onRuntimeException: (callback) => {
      client.on("Runtime.exceptionThrown", (event) => {
        callback(event.exceptionDetails?.exception?.description ?? event.exceptionDetails?.text ?? "unknown runtime exception");
      });
    },
    navigate: async (targetUrl) => {
      await client.send("Page.navigate", { url: targetUrl });
      await client.waitFor("Page.loadEventFired", 20_000);
    },
    evaluate: async (expression) => {
      const response = await client.send("Runtime.evaluate", {
        expression,
        awaitPromise: true,
        returnByValue: true
      });

      if (response.exceptionDetails) {
        throw new Error(response.exceptionDetails.exception?.description ?? response.exceptionDetails.text);
      }

      return response.result.value;
    },
    captureScreenshot: async () => {
      const response = await client.send("Page.captureScreenshot", { format: "png", fromSurface: true });
      return response.data;
    }
  };
}

class CdpClient {
  static async connect(url) {
    const socket = new WebSocket(url);
    const client = new CdpClient(socket);

    await new Promise((resolve, reject) => {
      socket.addEventListener("open", resolve, { once: true });
      socket.addEventListener("error", reject, { once: true });
    });

    return client;
  }

  constructor(socket) {
    this.socket = socket;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map();

    socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);

      if (message.id && this.pending.has(message.id)) {
        const { resolve, reject } = this.pending.get(message.id);
        this.pending.delete(message.id);

        if (message.error) {
          reject(new Error(message.error.message));
        } else {
          resolve(message.result);
        }
      }

      if (message.method && this.listeners.has(message.method)) {
        for (const listener of this.listeners.get(message.method)) {
          listener(message.params);
        }
      }
    });
  }

  send(method, params = {}) {
    const id = this.nextId++;

    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  on(method, callback) {
    if (!this.listeners.has(method)) {
      this.listeners.set(method, []);
    }

    this.listeners.get(method).push(callback);
  }

  waitFor(method, timeoutMs) {
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error(`Timed out waiting for ${method}`)), timeoutMs);

      this.on(method, (params) => {
        clearTimeout(timeout);
        resolve(params);
      });
    });
  }

  close() {
    this.socket.close();
  }
}

function runNpm(args) {
  return run(...npmSpawnArgs(args));
}

function npmSpawnArgs(args) {
  return isWindows ? ["cmd.exe", ["/d", "/s", "/c", "npm", ...args]] : ["npm", args];
}

function run(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { cwd: root, stdio: "inherit" });
    child.on("exit", (code) => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`${command} ${args.join(" ")} failed with exit code ${code}`));
      }
    });
  });
}

function terminateProcess(child) {
  if (!child?.pid || child.exitCode !== null) {
    return Promise.resolve();
  }

  if (isWindows) {
    return new Promise((resolve) => {
      const killer = spawn("taskkill.exe", ["/pid", String(child.pid), "/t", "/f"], {
        stdio: "ignore"
      });
      killer.on("exit", () => resolve());
      killer.on("error", () => resolve());
    });
  }

  child.kill("SIGTERM");

  return new Promise((resolve) => {
    child.on("exit", () => resolve());
    setTimeout(resolve, 2000);
  });
}

async function waitForHttp(url, timeoutMs) {
  const deadline = Date.now() + timeoutMs;

  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok) {
        return;
      }
    } catch {
      // Retry until the server is ready.
    }

    await new Promise((resolve) => setTimeout(resolve, 250));
  }

  throw new Error(`Timed out waiting for ${url}`);
}

function getFreePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      server.close(() => resolve(address.port));
    });
    server.on("error", reject);
  });
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
