// A minimal Chrome DevTools Protocol client for driving the real desktop app
// through WebView2's remote debugging port. Node 22+ (global WebSocket, fetch).

import { writeFile } from "node:fs/promises";

export async function connect(port = 9222, attempts = 60) {
  let target;
  for (let i = 0; i < attempts; i += 1) {
    try {
      const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
      target = targets.find((t) => t.type === "page" && !t.url.startsWith("devtools://"));
      if (target) break;
    } catch {
      // Not listening yet.
    }
    await sleep(500);
  }
  if (!target) throw new Error(`no page target on port ${port}`);

  const socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });

  let nextId = 0;
  const waiting = new Map();
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    const entry = waiting.get(message.id);
    if (!entry) return;
    waiting.delete(message.id);
    if (message.error) entry.reject(new Error(message.error.message));
    else entry.resolve(message.result);
  });

  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      nextId += 1;
      waiting.set(nextId, { resolve, reject });
      socket.send(JSON.stringify({ id: nextId, method, params }));
    });

  /** Evaluates an expression in the page and returns its JSON value. */
  const evaluate = async (expression) => {
    const result = await send("Runtime.evaluate", {
      expression,
      awaitPromise: true,
      returnByValue: true,
      userGesture: true,
    });
    if (result.exceptionDetails) {
      const detail = result.exceptionDetails;
      throw new Error(detail.exception?.description ?? detail.text);
    }
    return result.result.value;
  };

  /** Calls a Tauri command exactly as the UI does. Resolves to {ok} or {err}. */
  const invoke = (command, args = {}) =>
    evaluate(
      `window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})
         .then((ok) => ({ ok }), (err) => ({ err }))`,
    );

  const waitFor = async (expression, label = expression, timeoutMs = 15000) => {
    const started = Date.now();
    for (;;) {
      const value = await evaluate(expression).catch(() => null);
      if (value) return value;
      if (Date.now() - started > timeoutMs) throw new Error(`timed out waiting for: ${label}`);
      await sleep(100);
    }
  };

  /** Clicks the first element matching `selector` whose text contains `text`. */
  const click = async (selector, text = "") => {
    const found = await evaluate(`(() => {
      const wanted = ${JSON.stringify(text)};
      const nodes = [...document.querySelectorAll(${JSON.stringify(selector)})]
        .filter((n) => n.offsetParent !== null || n.getClientRects().length > 0);
      const node = nodes.find((n) => !wanted || (n.textContent ?? "").includes(wanted) || n.getAttribute("aria-label") === wanted);
      if (!node) return false;
      node.click();
      return true;
    })()`);
    if (!found) throw new Error(`nothing to click: ${selector} "${text}"`);
    await sleep(120);
  };

  /** Focuses a field, replaces its content, and types through the browser. */
  const type = async (selector, text) => {
    const ok = await evaluate(`(() => {
      const nodes = [...document.querySelectorAll(${JSON.stringify(selector)})]
        .filter((n) => n.offsetParent !== null);
      const node = nodes[0];
      if (!node) return false;
      node.focus();
      node.select?.();
      return true;
    })()`);
    if (!ok) throw new Error(`nothing to type into: ${selector}`);
    await send("Input.insertText", { text });
    await sleep(120);
  };

  const key = async (keyName, modifiers = 0) => {
    const base = { key: keyName, modifiers, windowsVirtualKeyCode: keyCode(keyName), code: keyName };
    await send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...base });
    await send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
    await sleep(80);
  };

  const screenshot = async (file) => {
    const { data } = await send("Page.captureScreenshot", { format: "png" });
    await writeFile(file, Buffer.from(data, "base64"));
  };

  const text = (selector = "body") =>
    evaluate(`(document.querySelector(${JSON.stringify(selector)})?.innerText ?? "")`);

  return { send, evaluate, invoke, waitFor, click, type, key, screenshot, text, close: () => socket.close() };
}

function keyCode(name) {
  const codes = { Enter: 13, Escape: 27, Tab: 9, ArrowDown: 40, ArrowUp: 38, ArrowRight: 39, ArrowLeft: 37 };
  if (codes[name]) return codes[name];
  return name.length === 1 ? name.toUpperCase().charCodeAt(0) : 0;
}

export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
export const CTRL = 2;
