// Drives the real worker (headless Chromium) through the practice forms,
// over HTTP, exactly as the Rust API does.

import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { classifyButton } from "../src/buttons.ts";
import { startServer, type Worker } from "../src/server.ts";
import type { Snapshot } from "../src/types.ts";

let worker: Worker;
let profileDir: string;

beforeAll(async () => {
  profileDir = await mkdtemp(join(tmpdir(), "cp-worker-"));
  worker = await startServer(0, { profileDir, headless: true });
});

afterAll(async () => {
  await worker?.close();
  await rm(profileDir, { recursive: true, force: true });
});

async function call<T>(method: string, path: string, body?: unknown): Promise<{ status: number; data: T }> {
  const res = await fetch(`${worker.url}${path}`, {
    method,
    headers: body ? { "Content-Type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  return { status: res.status, data: (await res.json()) as T };
}

const open = async (name: string) =>
  (await call<{ tab: string }>("POST", "/tabs", { url: `${worker.url}/practice/${name}` })).data.tab;
const snapshot = async (tab: string) => (await call<Snapshot>("GET", `/tabs/${tab}/snapshot`)).data;
const field = (snap: Snapshot, label: RegExp) => {
  const found = snap.fields.find((f) => label.test(f.label));
  if (!found) throw new Error(`no field ${label}: ${snap.fields.map((f) => f.label).join(" | ")}`);
  return found;
};
const button = (snap: Snapshot, text: RegExp) => snap.buttons.find((b) => text.test(b.text))!;

describe("buttons", () => {
  it("treats anything that could send the application as submit", () => {
    expect(classifyButton("Submit application")).toBe("submit");
    expect(classifyButton("Apply now")).toBe("submit");
    expect(classifyButton("Review and submit")).toBe("submit");
    expect(classifyButton("Send")).toBe("submit");
    expect(classifyButton("Continue")).toBe("next");
    expect(classifyButton("Save and continue")).toBe("next");
    expect(classifyButton("Back")).toBe("back");
    expect(classifyButton("Show more")).toBe("other");
  });
});

describe("quick apply practice form", () => {
  it("reads, fills, uploads and moves through the steps, then stops at Submit", async () => {
    const tab = await open("quick-apply");
    let snap = await snapshot(tab);

    expect(snap.captcha).toBe(false);
    expect(snap.password).toBe(false);
    // Like SEEK: saved resumés are the options, the old one is picked, and
    // each "Upload" button is reported as a file field under its heading.
    const resume = snap.fields.find((f) => f.label === "Resumé" && f.kind === "radio")!;
    expect(resume.options).toEqual(["Old CV 2024.pdf Added 20 days ago", "Don't include a resumé"]);
    expect(resume.value).toBe("Old CV 2024.pdf Added 20 days ago");
    // Each document has an invisible file box and an Upload button; both are
    // named after the heading above them.
    const uploads = snap.fields.filter((f) => f.kind === "file");
    expect(uploads.map((f) => f.label)).toEqual(["Resumé", "Cover letter", "Resumé", "Cover letter"]);
    expect(snap.buttons.some((b) => b.text === "Upload")).toBe(false);

    const upload = await call("POST", `/tabs/${tab}/upload`, {
      id: uploads[0].id,
      filename: "Test Person - Resume - Practice Co.docx",
      content_base64: Buffer.from("not really a docx").toString("base64"),
    });
    expect(upload.status).toBe(200);
    snap = await snapshot(tab);
    // The new file joins the list, picked. Ids stay the same between snapshots.
    const after = snap.fields.find((f) => f.label === "Resumé" && f.kind === "radio")!;
    expect(after.id).toBe(resume.id);
    expect(after.value).toBe("Test Person - Resume - Practice Co.docx Added just now");
    expect(after.options).toHaveLength(3);

    // The Upload button works too (through the file picker it opens).
    const letterButton = uploads[3];
    const viaButton = await call("POST", `/tabs/${tab}/upload`, {
      id: letterButton.id,
      filename: "Test Person - Cover letter - Practice Co.docx",
      content_base64: Buffer.from("not really a docx").toString("base64"),
    });
    expect(viaButton.status).toBe(200);
    snap = await snapshot(tab);
    expect(snap.fields.find((f) => f.label === "Cover letter" && f.kind === "radio")!.value).toBe(
      "Test Person - Cover letter - Practice Co.docx Added just now",
    );

    await call("POST", `/tabs/${tab}/click`, { id: button(snap, /Continue/).id });
    snap = await snapshot(tab);
    expect(snap.headings).toContain("Answer employer questions");
    const rights = field(snap, /right to work/);
    expect(rights.kind).toBe("select");
    expect(rights.options).toContain("I have a temporary graduate visa with full work rights");
    expect(rights.options).not.toContain("Select an option");
    expect(field(snap, /Power BI/).options).toEqual(["Yes", "No"]);

    // Continue without answering: the page shows an error and stays.
    await call("POST", `/tabs/${tab}/click`, { id: button(snap, /Continue/).id });
    snap = await snapshot(tab);
    expect(snap.errors.join(" ")).toMatch(/Please answer/);

    const fills = await call<{ results: { ok: boolean; error?: string }[] }>("POST", `/tabs/${tab}/fill`, {
      fields: [
        { id: rights.id, value: "I have a temporary graduate visa with full work rights" },
        { id: field(snap, /years' experience/).id, value: "2 years" },
        { id: field(snap, /Power BI/).id, value: "yes" },
        { id: field(snap, /Working With Children/).id, value: "No" },
        { id: field(snap, /salary/).id, value: "$90k" },
        { id: field(snap, /salary/).id, value: "a million dollars" },
      ],
    });
    expect(fills.data.results.map((r) => r.ok)).toEqual([true, true, true, true, true, false]);
    expect(fills.data.results[5].error).toMatch(/no option/);

    await call("POST", `/tabs/${tab}/click`, { id: button(snap, /Continue/).id });
    snap = await snapshot(tab);
    expect(snap.headings).toContain("Update your profile");
    expect(field(snap, /role title/).value).toBe("Data Analyst");

    await call("POST", `/tabs/${tab}/click`, { id: button(snap, /Continue/).id });
    snap = await snapshot(tab);
    const submit = button(snap, /Submit application/);
    expect(submit.kind).toBe("submit");
    expect(snap.buttons.some((b) => b.kind === "next")).toBe(false);

    // The worker refuses to click it.
    const blocked = await call<{ code: string }>("POST", `/tabs/${tab}/click`, { id: submit.id });
    expect(blocked.status).toBe(409);
    expect(blocked.data.code).toBe("submit_blocked");
    snap = await snapshot(tab);
    expect(snap.headings).toContain("Review and submit");

    await call("DELETE", `/tabs/${tab}`);
    expect((await call("GET", `/tabs/${tab}`)).status).toBe(404);
  });
});

describe("employer practice form", () => {
  it("finds labels from <label for>, required stars and the privacy checkbox", async () => {
    const tab = await open("employer-form");
    const snap = await snapshot(tab);
    expect(field(snap, /^First name/).required).toBe(true);
    expect(field(snap, /^Email/).kind).toBe("email");
    expect(field(snap, /^Mobile/).kind).toBe("tel");
    expect(field(snap, /^LinkedIn/).required).toBe(false);
    expect(field(snap, /CV/).kind).toBe("file");
    expect(field(snap, /interested/).kind).toBe("textarea");
    const privacy = field(snap, /privacy policy/);
    expect(privacy.kind).toBe("checkbox");
    expect(privacy.value).toBe("No");

    await call("POST", `/tabs/${tab}/fill`, {
      fields: [
        { id: field(snap, /^First name/).id, value: "Test" },
        { id: privacy.id, value: "Yes" },
      ],
    });
    const after = await snapshot(tab);
    expect(field(after, /^First name/).value).toBe("Test");
    expect(field(after, /privacy policy/).value).toBe("Yes");
    await call("DELETE", `/tabs/${tab}`);
  });
});

describe("captcha", () => {
  it("notices a CAPTCHA box", async () => {
    const tab = await open("captcha");
    expect((await snapshot(tab)).captcha).toBe(true);
    await call("DELETE", `/tabs/${tab}`);
  });

  it("answers 404 for a tab that doesn't exist", async () => {
    const res = await call<{ code: string }>("GET", "/tabs/t999/snapshot");
    expect(res.status).toBe(404);
    expect(res.data.code).toBe("tab_not_found");
  });
});
