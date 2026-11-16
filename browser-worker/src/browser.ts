import { chromium, type BrowserContext, type Locator, type Page } from "playwright";

import { classifyButton } from "./buttons.ts";
import { readPage } from "./page-script.ts";
import type { FillRequest, FillResult, SavedResume, Snapshot } from "./types.ts";

export interface BrowserOptions {
  /** Where the browser keeps its cookies, so your SEEK login survives restarts. */
  profileDir: string;
  /** true in tests and CI. On your PC the window is visible. */
  headless: boolean;
}

export class TabNotFound extends Error {}
export class SubmitBlocked extends Error {}

/**
 * One Chromium window with its own profile (not your everyday Chrome).
 * Each application runs in its own tab, so several can run at once.
 */
export class Browser {
  private readonly options: BrowserOptions;
  private context: BrowserContext | undefined;
  private starting: Promise<BrowserContext> | undefined;
  private readonly tabs = new Map<string, Page>();
  /** A separate, always headless Chromium for PDFs (only headless Chromium prints them). */
  private printer: Promise<import("playwright").Browser> | undefined;
  private nextTab = 1;

  constructor(options: BrowserOptions) {
    this.options = options;
  }

  private async ensureContext(): Promise<BrowserContext> {
    if (this.context) return this.context;
    // Two requests at once must not launch two browsers on one profile.
    this.starting ??= chromium
      .launchPersistentContext(this.options.profileDir, {
        headless: this.options.headless,
        viewport: this.options.headless ? { width: 1280, height: 900 } : null,
        args: this.options.headless
          ? []
          : [
              "--start-maximized",
              // The window spends most of its time minimised: keep it at full speed anyway.
              "--disable-backgrounding-occluded-windows",
              "--disable-renderer-backgrounding",
              "--disable-background-timer-throttling",
            ],
      })
      .then(async (context) => {
        context.on("close", () => {
          // You closed the window: start a fresh one next time.
          this.context = undefined;
          this.tabs.clear();
        });
        // The agent works out of sight. You open the window when you want it:
        // "Show in the browser" in the app, or its icon on the taskbar.
        const first = context.pages()[0];
        if (first) await this.windowState(first, "minimized");
        this.context = context;
        return context;
      })
      .finally(() => {
        this.starting = undefined;
      });
    return this.starting;
  }

  get headless(): boolean {
    return this.options.headless;
  }

  get openTabs(): number {
    return this.tabs.size;
  }

  /** Open a tab. It stays out of sight unless `show` (or you already have the window open). */
  async open(url: string, show = false): Promise<string> {
    const context = await this.ensureContext();
    const current = context.pages().find((p) => !p.isClosed());
    const wasHidden = current ? (await this.windowState(current)) === "minimized" : true;
    const page = await context.newPage();
    if (show) await this.show(page);
    else if (wasHidden) await this.windowState(page, "minimized");
    const id = `t${this.nextTab++}`;
    this.tabs.set(id, page);
    page.on("close", () => this.tabs.delete(id));
    await page.goto(url, { waitUntil: "domcontentloaded", timeout: 45_000 });
    await settle(page);
    return id;
  }

  private page(tab: string): Page {
    const page = this.tabs.get(tab);
    if (!page || page.isClosed()) throw new TabNotFound(tab);
    return page;
  }

  has(tab: string): boolean {
    const page = this.tabs.get(tab);
    return Boolean(page && !page.isClosed());
  }

  url(tab: string): string {
    return this.page(tab).url();
  }

  async snapshot(tab: string): Promise<Snapshot> {
    const page = this.page(tab);
    const snap = await readWhenStill(page);
    return {
      ...snap,
      buttons: snap.buttons.map((b) => ({ ...b, kind: classifyButton(b.text) })),
    };
  }

  async fill(tab: string, fields: FillRequest[]): Promise<FillResult[]> {
    const page = this.page(tab);
    const hidden = await this.hidden(page);
    const results: FillResult[] = [];
    for (const field of fields) {
      try {
        await fillOne(page, field, hidden);
        results.push({ id: field.id, ok: true });
      } catch (error) {
        results.push({ id: field.id, ok: false, error: message(error) });
      }
    }
    return results;
  }

  async upload(tab: string, id: string, filename: string, content: Buffer): Promise<void> {
    const page = this.page(tab);
    const target = page.locator(`[data-cp="${id}"]`).first();
    const file = { name: filename, mimeType: mimeFor(filename), buffer: content };
    const isFileBox = await target.evaluate((e) => e instanceof HTMLInputElement && e.type === "file");
    if (isFileBox) {
      await target.setInputFiles(file);
    } else {
      // An "Upload" button (SEEK): click it and answer the file picker it opens.
      const text = await target.innerText().catch(() => "");
      if (classifyButton(text) === "submit") {
        throw new SubmitBlocked(`"${text.trim()}" looks like it sends the application, so it's yours to click.`);
      }
      const pick = (hidden: boolean) =>
        Promise.all([page.waitForEvent("filechooser", { timeout: 10_000 }), press(target, hidden)]);
      // If a site only opens its file picker for a real click, use one (the
      // window pops up for that one click).
      const [chooser] = (await this.hidden(page)) ? await pick(true).catch(() => pick(false)) : await pick(false);
      await chooser.setFiles(file);
    }
    await settle(page);
  }

  /**
   * Click a Next / Continue (or other) button. Never a Submit button, except
   * with `auth` on a login, sign-up or code page (a password or code box is
   * showing): there "Submit" signs you in, it doesn't send an application.
   */
  async click(tab: string, id: string, auth = false): Promise<string> {
    const page = this.page(tab);
    const button = page.locator(`[data-cp-btn="${id}"]`).first();
    const text = (await button.innerText().catch(() => "")) || (await button.inputValue().catch(() => ""));
    if (classifyButton(text) === "submit" && !(auth && (await page.evaluate(isAuthPage, text)))) {
      throw new SubmitBlocked(`"${text.trim()}" looks like it sends the application, so it's yours to click.`);
    }
    await press(button, await this.hidden(page));
    await page.waitForLoadState("domcontentloaded").catch(() => {});
    await settle(page);
    return page.url();
  }

  /** Go to a link in this tab, e.g. the "verify your email" link from an email. */
  async goto(tab: string, url: string): Promise<string> {
    const page = this.page(tab);
    await page.goto(url, { waitUntil: "domcontentloaded", timeout: 45_000 });
    await settle(page);
    return page.url();
  }

  /**
   * The resumés saved on a SEEK-style profile page whose file name starts
   * with one of `prefixes` ("Jane Citizen - Resume"), in page order, with the
   * "Added 3 days ago" text beside each.
   */
  async savedResumes(tab: string, prefixes: string[]): Promise<SavedResume[]> {
    const found = await this.page(tab).evaluate(findResumes, prefixes);
    return found.map(({ name, added, index }) => ({ name, added, index }));
  }

  /**
   * Delete the saved resumé at `index` (from savedResumes), after checking
   * it's still the one called `name`: its Delete button, then "Delete" in the
   * confirmation box. False if it couldn't do it safely.
   */
  async deleteResume(tab: string, prefixes: string[], index: number, name: string): Promise<boolean> {
    const page = this.page(tab);
    const before = await page.evaluate(findResumes, prefixes);
    const target = before[index];
    if (!target || target.name !== name || !target.button) return false;
    await page.locator(`[data-cp-del="${target.button}"]`).first().click({ timeout: 10_000 });
    // Only ever inside the confirmation box, so it can't hit another resumé's button.
    const confirm = page
      .locator('[role="dialog"], [role="alertdialog"], [aria-modal="true"]')
      .getByRole("button", { name: /^(delete|remove|yes)\b/i })
      .last();
    await confirm.click({ timeout: 5_000 }).catch(() => {});
    await settle(page);
    const after = await page.evaluate(findResumes, prefixes);
    return after.length === before.length - 1;
  }

  /**
   * Print an HTML document (a resumé or cover letter) to an A4 PDF, at
   * `scale` (0.8 to 1). Returns the PDF and its page count.
   */
  async pdf(html: string, scale = 1): Promise<{ pdf: Buffer; pages: number }> {
    this.printer ??= chromium.launch({ headless: true });
    const printer = await this.printer;
    const page = await printer.newPage();
    try {
      await page.setContent(html, { waitUntil: "load" });
      const pdf = await page.pdf({
        format: "A4",
        printBackground: true,
        scale: Math.min(1, Math.max(0.8, scale)),
        margin: { top: "14mm", bottom: "14mm", left: "16mm", right: "16mm" },
      });
      return { pdf, pages: countPages(pdf) };
    } finally {
      await page.close();
    }
  }

  /** Open the window on this tab, e.g. when it's ready for you to Submit. */
  async focus(tab: string): Promise<void> {
    await this.show(this.page(tab));
  }

  private async show(page: Page): Promise<void> {
    await page.bringToFront();
    // A minimised window has to be restored before it can be maximised.
    await this.windowState(page, "normal");
    await this.windowState(page, "maximized");
  }

  /** True while the window is minimised (the agent is working out of sight). */
  private async hidden(page: Page): Promise<boolean> {
    return (await this.windowState(page)) === "minimized";
  }

  /**
   * Read (and optionally set) the window's state through Chrome's own
   * protocol: "minimized", "normal" or "maximized". Nothing to do headless.
   */
  private async windowState(
    page: Page,
    state?: "minimized" | "normal" | "maximized",
  ): Promise<string | undefined> {
    if (this.options.headless) return undefined;
    const cdp = await page.context().newCDPSession(page);
    try {
      const { windowId, bounds } = await cdp.send("Browser.getWindowForTarget");
      if (state && bounds.windowState !== state) {
        await cdp.send("Browser.setWindowBounds", { windowId, bounds: { windowState: state } });
      }
      return state ?? bounds.windowState;
    } catch {
      return undefined; // Not worth failing a run over.
    } finally {
      await cdp.detach().catch(() => {});
    }
  }

  async close(tab: string): Promise<void> {
    const page = this.tabs.get(tab);
    this.tabs.delete(tab);
    await page?.close().catch(() => {});
  }

  async shutdown(): Promise<void> {
    if (this.printer) await (await this.printer).close().catch(() => {});
    this.printer = undefined;
    await this.context?.close().catch(() => {});
  }
}

/**
 * Runs in the page: is this a sign-in step rather than the application?
 * True when a password or one-time code box is showing, or when the only
 * box on the page is your email (a "sign in with a code" page). A button
 * that mentions the application never counts.
 */
function isAuthPage(buttonText: string): boolean {
  if (/appl/i.test(buttonText)) return false;
  const visible = (el: Element) => {
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== "hidden";
  };
  const inputs = Array.from(
    document.querySelectorAll<HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement>("input, textarea, select"),
  ).filter(
    (el) =>
      visible(el) &&
      !(el instanceof HTMLInputElement && ["hidden", "checkbox", "radio", "submit", "button"].includes(el.type)),
  );
  const code = /code|otp|one.?time|passcode/i;
  const secret = inputs.some(
    (el) =>
      el instanceof HTMLInputElement &&
      (el.type === "password" ||
        el.autocomplete === "one-time-code" ||
        (code.test(`${el.name} ${el.id} ${el.placeholder} ${el.getAttribute("aria-label") ?? ""}`) &&
          !/post|zip|promo|area/i.test(`${el.name} ${el.id} ${el.placeholder}`))),
  );
  const onlyEmail =
    inputs.length === 1 &&
    inputs[0] instanceof HTMLInputElement &&
    (inputs[0].type === "email" || /email|username/i.test(`${inputs[0].autocomplete} ${inputs[0].name} ${inputs[0].id}`));
  return secret || onlyEmail;
}

/**
 * Runs in the page: saved resumés whose name starts with one of `prefixes`,
 * each with the Delete / Remove button of its own row (tagged data-cp-del)
 * and the rest of the row's text ("Added 3 days ago").
 */
function findResumes(prefixes: string[]): { name: string; added: string; index: number; button: string | null }[] {
  const w = window as unknown as { __cpDel?: number };
  const shown = (el: HTMLElement) => el.getBoundingClientRect().width > 0;
  const text = (el: HTMLElement) => (el.innerText ?? "").replace(/\s+/g, " ").trim();
  const starts = (t: string) => prefixes.some((p) => t.startsWith(p));
  const isDelete = (b: HTMLElement) =>
    /^(delete|remove)\b/i.test((b.innerText || b.getAttribute("aria-label") || "").trim());

  // The smallest visible elements whose text is a file name we're after.
  const holders = Array.from(document.querySelectorAll<HTMLElement>("body *")).filter(
    (el) =>
      shown(el) &&
      starts(text(el)) &&
      !Array.from(el.children).some((c) => starts(text(c as HTMLElement))),
  );
  return holders.map((holder, index) => {
    // Walk up until the box holds exactly one Delete / Remove button: that
    // box is this resumé's row. If it ever holds more than one, it's another
    // resumé's too, so there's no safe button to press.
    let button: HTMLElement | null = null;
    let row: HTMLElement = holder;
    for (let box: HTMLElement | null = holder; box && box !== document.body; box = box.parentElement) {
      const found = Array.from(box.querySelectorAll<HTMLElement>('button, a, [role="button"]')).filter(isDelete);
      if (found.length === 1) {
        button = found[0];
        row = box;
        break;
      }
      if (found.length > 1) break;
    }
    let id: string | null = null;
    if (button) {
      id = button.dataset.cpDel || `d${(w.__cpDel = (w.__cpDel ?? 0) + 1)}`;
      button.dataset.cpDel = id;
    }
    const name = text(holder);
    const added = text(row).replace(name, "").replace(/\b(delete|remove)\b/gi, "").trim();
    return { name, added, index, button: id };
  });
}

/**
 * Click something. A real mouse click pops a minimised window back up, so
 * while the window is out of sight we click through the page instead, like a
 * script on the page would. Once you have the window open, it's a real click.
 */
async function press(target: Locator, hidden: boolean): Promise<void> {
  if (!hidden) {
    await target.click({ timeout: 10_000 });
    return;
  }
  await target.evaluate((el) => {
    el.scrollIntoView({ block: "center" });
    (el as HTMLElement).click();
  });
}

async function fillOne(page: Page, { id, value }: FillRequest, hidden: boolean): Promise<void> {
  const all = page.locator(`[data-cp="${id}"]`);
  const count = await all.count();
  if (count === 0) throw new Error("field not found");
  const first = all.first();
  const { tag, type } = await first.evaluate((el) => ({
    tag: el.tagName.toLowerCase(),
    type: ((el as HTMLInputElement).type || "").toLowerCase(),
  }));

  if (type === "radio") {
    for (let i = 0; i < count; i++) {
      const option = all.nth(i);
      const label = (await option.getAttribute("data-cp-label")) ?? "";
      if (same(label, value)) {
        if (!hidden) await option.check({ force: true });
        else if (!(await option.isChecked())) await press(option, true);
        return;
      }
    }
    throw new Error(`no option "${value}"`);
  }
  if (type === "checkbox") {
    const want = /^(yes|true|on|1)$/i.test(value.trim());
    if (!hidden) await first.setChecked(want, { force: true });
    else if ((await first.isChecked()) !== want) await press(first, true);
    return;
  }
  if (tag === "select") {
    const options = await first.evaluate((el) =>
      Array.from((el as HTMLSelectElement).options).map((o) => ({
        value: o.value,
        label: (o.textContent ?? "").replace(/\s+/g, " ").trim(),
      })),
    );
    const match = options.find((o) => o.value !== "" && same(o.label, value));
    if (!match) throw new Error(`no option "${value}"`);
    await first.selectOption(match.value);
    return;
  }
  if (type === "file") throw new Error("use upload for files");

  await first.fill(value);
  await first.evaluate((el) => {
    // Some forms only validate on blur.
    el.dispatchEvent(new Event("change", { bubbles: true }));
    (el as HTMLElement).blur();
  });
}

const normalise = (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
const same = (a: string, b: string) => normalise(a) === normalise(b);

/** Give the page a moment to react (client-side steps, validation messages). */
/**
 * Read the page, waiting it out if it navigates meanwhile (a redirect after
 * signing in, a page that reloads itself). Up to 5 tries, about 10 seconds.
 */
async function readWhenStill(page: Page) {
  for (let attempt = 1; ; attempt++) {
    try {
      await page.waitForLoadState("domcontentloaded", { timeout: 10_000 }).catch(() => {});
      return await page.evaluate(readPage);
    } catch (error) {
      const moving = /context was destroyed|navigat|Cannot find context/i.test(String(error));
      if (!moving || attempt >= 5) throw error;
      await settle(page);
    }
  }
}

async function settle(page: Page): Promise<void> {
  await page.waitForLoadState("networkidle", { timeout: 3_000 }).catch(() => {});
  await page.waitForTimeout(300);
}

/** Pages in a PDF: its "/Type /Page" objects (not "/Type /Pages"). */
export function countPages(pdf: Buffer): number {
  return (pdf.toString("latin1").match(/\/Type\s*\/Page(?![a-z])/g) ?? []).length;
}

function mimeFor(filename: string): string {
  if (filename.endsWith(".docx")) {
    return "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
  }
  if (filename.endsWith(".pdf")) return "application/pdf";
  return "application/octet-stream";
}

export function message(error: unknown): string {
  return error instanceof Error ? error.message.split("\n")[0] : String(error);
}
