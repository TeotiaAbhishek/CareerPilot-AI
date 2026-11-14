// A small HTTP API over the browser, for the Rust backend. It only listens on
// 127.0.0.1, so nothing else on your network can drive your browser.
//
//   GET    /health                    is the worker up?
//   POST   /tabs              {url, show?} open a tab (out of sight unless show) -> {tab}
//   GET    /tabs/:tab                 does the tab still exist? -> {url}
//   GET    /tabs/:tab/snapshot        read the form          -> Snapshot
//   POST   /tabs/:tab/fill   {fields} fill fields            -> {results}
//   POST   /tabs/:tab/upload {id, filename, content_base64}
//   POST   /tabs/:tab/click  {id}     click Next / Continue (never Submit)
//   POST   /tabs/:tab/focus           open the window on this tab
//   DELETE /tabs/:tab                 close it
//   GET    /practice/:name            practice forms (no real site involved)

import { readFile } from "node:fs/promises";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";

import { Browser, SubmitBlocked, TabNotFound, message, type BrowserOptions } from "./browser.ts";

const MAX_BODY = 10 * 1024 * 1024;
const PRACTICE = new Set(["quick-apply", "employer-form", "captcha"]);

export interface Worker {
  url: string;
  browser: Browser;
  close(): Promise<void>;
}

class HttpError extends Error {
  readonly status: number;
  constructor(status: number, text: string) {
    super(text);
    this.status = status;
  }
}

export async function startServer(
  port: number,
  options: BrowserOptions,
  host = "127.0.0.1",
): Promise<Worker> {
  const browser = new Browser(options);

  const server = createServer((req, res) => {
    handle(browser, req, res).catch((error: unknown) => {
      const status =
        error instanceof HttpError
          ? error.status
          : error instanceof TabNotFound
            ? 404
            : error instanceof SubmitBlocked
              ? 409
              : 500;
      const code =
        error instanceof TabNotFound
          ? "tab_not_found"
          : error instanceof SubmitBlocked
            ? "submit_blocked"
            : undefined;
      if (status === 500) console.error(error);
      send(res, status, { error: message(error), code });
    });
  });

  await new Promise<void>((resolve) => server.listen(port, host, resolve));
  const address = server.address() as AddressInfo;
  return {
    url: `http://${host}:${address.port}`,
    browser,
    async close() {
      await browser.shutdown();
      await new Promise<void>((resolve) => server.close(() => resolve()));
    },
  };
}

async function handle(browser: Browser, req: IncomingMessage, res: ServerResponse) {
  const url = new URL(req.url ?? "/", "http://localhost");
  const parts = url.pathname.split("/").filter(Boolean);
  const method = req.method ?? "GET";

  if (method === "GET" && url.pathname === "/health") {
    return send(res, 200, { ok: true, headless: browser.headless, tabs: browser.openTabs });
  }

  if (method === "GET" && parts[0] === "practice" && parts.length === 2) {
    if (!PRACTICE.has(parts[1])) throw new HttpError(404, "No such practice form.");
    const html = await readFile(new URL(`../practice/${parts[1]}.html`, import.meta.url));
    res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
    return res.end(html);
  }

  if (parts[0] !== "tabs") throw new HttpError(404, "Not found.");

  if (parts.length === 1 && method === "POST") {
    const { url: target, show } = await body<{ url?: string; show?: boolean }>(req);
    if (!target || !/^https?:\/\//.test(target)) throw new HttpError(400, "url must start with http(s)://");
    const tab = await browser.open(target, show === true);
    return send(res, 201, { tab, url: browser.url(tab) });
  }

  const tab = parts[1];
  const action = parts[2];
  if (!tab) throw new HttpError(404, "Not found.");

  if (!action && method === "GET") {
    if (!browser.has(tab)) throw new TabNotFound(tab);
    return send(res, 200, { tab, url: browser.url(tab) });
  }
  if (!action && method === "DELETE") {
    await browser.close(tab);
    return send(res, 200, { ok: true });
  }
  if (action === "snapshot" && method === "GET") {
    return send(res, 200, await browser.snapshot(tab));
  }
  if (method !== "POST") throw new HttpError(405, "Method not allowed.");

  switch (action) {
    case "fill": {
      const { fields } = await body<{ fields?: { id: string; value: string }[] }>(req);
      if (!Array.isArray(fields)) throw new HttpError(400, "fields must be a list");
      return send(res, 200, { results: await browser.fill(tab, fields) });
    }
    case "upload": {
      const input = await body<{ id?: string; filename?: string; content_base64?: string }>(req);
      if (!input.id || !input.filename || !input.content_base64) {
        throw new HttpError(400, "id, filename and content_base64 are needed");
      }
      await browser.upload(tab, input.id, input.filename, Buffer.from(input.content_base64, "base64"));
      return send(res, 200, { ok: true });
    }
    case "click": {
      const { id } = await body<{ id?: string }>(req);
      if (!id) throw new HttpError(400, "id is needed");
      return send(res, 200, { ok: true, url: await browser.click(tab, id) });
    }
    case "focus":
      await browser.focus(tab);
      return send(res, 200, { ok: true });
    default:
      throw new HttpError(404, "Not found.");
  }
}

async function body<T>(req: IncomingMessage): Promise<T> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of req) {
    size += (chunk as Buffer).length;
    if (size > MAX_BODY) throw new HttpError(413, "Body too large.");
    chunks.push(chunk as Buffer);
  }
  if (size === 0) return {} as T;
  try {
    return JSON.parse(Buffer.concat(chunks).toString("utf8")) as T;
  } catch {
    throw new HttpError(400, "Body must be JSON.");
  }
}

function send(res: ServerResponse, status: number, data: unknown) {
  res.writeHead(status, { "Content-Type": "application/json" });
  res.end(JSON.stringify(data));
}
