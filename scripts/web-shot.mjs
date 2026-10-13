// Opens a page in headless Firefox, waits, and saves a screenshot: the web
// counterpart of `sg-app --screenshot`, for checking the web build without a
// person at the browser. Console errors from the page are printed.
//
//   node scripts/web-shot.mjs <url> <out.png> [seconds-to-wait]
//
// Needs Firefox and Node.js 22+ (built-in WebSocket). Uses a throwaway
// profile, so it does not touch a running Firefox. Talks WebDriver BiDi.

import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync, existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const [url, out, waitArg] = process.argv.slice(2);
if (!url || !out) {
  console.error("usage: node scripts/web-shot.mjs <url> <out.png> [seconds-to-wait]");
  process.exit(2);
}
const waitSeconds = Number(waitArg ?? 12);
const port = 9200 + Math.floor(Math.random() * 500);

const candidates = [
  process.env.FIREFOX,
  "/Applications/Firefox.app/Contents/MacOS/firefox",
  "firefox",
].filter(Boolean);
const firefox = candidates.find((path) => !path.includes("/") || existsSync(path));

const profile = mkdtempSync(join(tmpdir(), "stardust-web-shot-"));
const browser = spawn(
  firefox,
  ["--headless", "--no-remote", "--profile", profile, "--remote-debugging-port", String(port),
   "--width", "1440", "--height", "900"],
  { stdio: "ignore" },
);

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function connect() {
  for (let attempt = 0; attempt < 100; attempt++) {
    try {
      return await new Promise((resolve, reject) => {
        const socket = new WebSocket(`ws://127.0.0.1:${port}/session`);
        socket.onopen = () => resolve(socket);
        socket.onerror = () => reject(new Error("not ready"));
      });
    } catch {
      await sleep(200);
    }
  }
  throw new Error("Firefox did not open its remote-control port");
}

let status = 0;
try {
  const socket = await connect();
  const pending = new Map();
  let nextId = 1;
  socket.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.type === "event" && message.method === "log.entryAdded") {
      const entry = message.params;
      if (entry.level === "error" || entry.level === "warn") {
        console.log(`[page ${entry.level}] ${entry.text}`);
      }
    } else if (pending.has(message.id)) {
      const { resolve, reject } = pending.get(message.id);
      pending.delete(message.id);
      if (message.type === "error") reject(new Error(`${message.error}: ${message.message}`));
      else resolve(message.result);
    }
  };
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const id = nextId++;
      pending.set(id, { resolve, reject });
      socket.send(JSON.stringify({ id, method, params }));
    });

  await send("session.new", { capabilities: {} });
  await send("session.subscribe", { events: ["log.entryAdded"] });
  const tree = await send("browsingContext.getTree");
  const context = tree.contexts[0].context;
  await send("browsingContext.navigate", { context, url, wait: "complete" });
  await sleep(waitSeconds * 1000);

  const facts = await send("script.evaluate", {
    expression: `JSON.stringify({
      renderer: document.documentElement.dataset.renderer ?? null,
      status: document.getElementById("status")?.className ?? null,
      message: document.getElementById("message")?.textContent ?? null,
      canvas: [document.querySelector("canvas")?.width, document.querySelector("canvas")?.height],
    })`,
    target: { context },
    awaitPromise: false,
  });
  console.log(`page state: ${facts.result?.value}`);

  const shot = await send("browsingContext.captureScreenshot", { context });
  writeFileSync(out, Buffer.from(shot.data, "base64"));
  console.log(`saved ${out}`);
  socket.close();
} catch (error) {
  console.error(`web-shot: ${error.message}`);
  status = 1;
} finally {
  browser.kill();
  await sleep(300);
  rmSync(profile, { recursive: true, force: true });
}
process.exit(status);
