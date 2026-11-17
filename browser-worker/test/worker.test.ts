// Drives the real worker (headless Chromium) through the practice forms,
// over HTTP, exactly as the Rust API does.

import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterAll, beforeAll, describe, expect, it } from "vitest";

import { classifyButton, isStartButton } from "../src/buttons.ts";
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
    // Only a job ad (no form on the page) turns "Apply now" into a way in.
    expect(isStartButton("Apply now")).toBe(true);
    expect(isStartButton("Submit application")).toBe(false);
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

describe("job ad on a careers site", () => {
  it("follows a redirect chain, then opens the form from the ad's Apply now link", async () => {
    const tab = await open("redirect?hops=3");
    const ad = await snapshot(tab);
    expect(ad.url).toMatch(/careers-ad/);
    expect(ad.fields).toHaveLength(0);
    const apply = button(ad, /^Apply now$/);
    expect(apply.kind).toBe("start");

    const res = await call<{ url: string }>("POST", `/tabs/${tab}/click`, { id: apply.id });
    expect(res.status).toBe(200);
    expect(res.data.url).toMatch(/employer-form/);
    const form = await snapshot(tab);
    expect(field(form, /^First name/).required).toBe(true);
    expect(button(form, /Submit application/).kind).toBe("submit");
    await call("DELETE", `/tabs/${tab}`);
  });

  it("carries on in the new tab when the ad opens the form there", async () => {
    const tab = await open("careers-ad#newtab");
    const apply = button(await snapshot(tab), /^Apply now$/);
    const res = await call<{ url: string }>("POST", `/tabs/${tab}/click`, { id: apply.id });
    expect(res.data.url).toMatch(/employer-form/);
    expect((await snapshot(tab)).url).toMatch(/employer-form/);
    await call("DELETE", `/tabs/${tab}`);
  });

  it("never clicks Apply now on a page that has a form: there it sends it", async () => {
    const tab = await open("apply-form");
    const snap = await snapshot(tab);
    const apply = button(snap, /^Apply now$/);
    expect(apply.kind).toBe("submit");
    const res = await call<{ code: string }>("POST", `/tabs/${tab}/click`, { id: apply.id });
    expect(res.status).toBe(409);
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

describe("careers site with a login", () => {
  it("signs up, reads the emailed code, and still never clicks Submit", async () => {
    const tab = (
      await call<{ tab: string }>("POST", "/tabs", { url: `${worker.url}/practice/careers-login?reset=1` })
    ).data.tab;
    let snap = await snapshot(tab);
    expect(snap.secrets).toHaveLength(1); // the sign-in password box
    expect(snap.text).toMatch(/sign in/i);
    await call("POST", `/tabs/${tab}/click`, { id: button(snap, /create an account/i).id });

    snap = await snapshot(tab);
    expect(snap.secrets).toHaveLength(2); // password and confirm
    const [password, confirm] = snap.secrets;
    await call("POST", `/tabs/${tab}/fill`, {
      fields: [
        { id: field(snap, /^First name/).id, value: "Test" },
        { id: field(snap, /^Last name/).id, value: "Person" },
        { id: field(snap, /^Email/).id, value: "test@example.com" },
        { id: password, value: "Practice-Pass-123" },
        { id: confirm, value: "Practice-Pass-123" },
        { id: field(snap, /Terms of Use/).id, value: "Yes" },
      ],
    });
    const created = await call("POST", `/tabs/${tab}/click`, { id: button(snap, /^create account/i).id, auth: true });
    expect(created.status).toBe(200);

    const mail = await call<{ emails: { body: string }[] }>("GET", "/practice/mailbox");
    const code = /\b(\d{6})\b/.exec(mail.data.emails.at(-1)!.body)![1];
    snap = await snapshot(tab);
    await call("POST", `/tabs/${tab}/fill`, { fields: [{ id: field(snap, /Verification code/).id, value: code }] });
    await call("POST", `/tabs/${tab}/click`, { id: button(snap, /verify/i).id, auth: true });

    snap = await snapshot(tab);
    expect(field(snap, /Why do you want/).kind).toBe("textarea");
    // No password or code box on the form, so "auth" doesn't unlock Submit.
    const blocked = await call("POST", `/tabs/${tab}/click`, { id: button(snap, /submit application/i).id, auth: true });
    expect(blocked.status).toBe(409);
    await call("DELETE", `/tabs/${tab}`);
  });
});

describe("careers site with email-code sign-in", () => {
  it("lets the agent send the email-only sign-in, then the code", async () => {
    const tab = await open("careers-code");
    let snap = await snapshot(tab);
    expect(snap.secrets).toHaveLength(0);
    await call("POST", `/tabs/${tab}/fill`, { fields: [{ id: field(snap, /^Email/).id, value: "test@example.com" }] });
    const sent = await call("POST", `/tabs/${tab}/click`, { id: button(snap, /email me/i).id, auth: true });
    expect(sent.status).toBe(200);

    const mail = await call<{ emails: { body: string }[] }>("GET", "/practice/mailbox");
    const code = /\b(\d{6})\b/.exec(mail.data.emails.at(-1)!.body)![1];
    snap = await snapshot(tab);
    await call("POST", `/tabs/${tab}/fill`, { fields: [{ id: field(snap, /Sign-in code/).id, value: code }] });
    await call("POST", `/tabs/${tab}/click`, { id: button(snap, /^sign in/i).id, auth: true });

    snap = await snapshot(tab);
    expect(field(snap, /Why do you want/).kind).toBe("textarea");
    const blocked = await call("POST", `/tabs/${tab}/click`, { id: button(snap, /submit application/i).id, auth: true });
    expect(blocked.status).toBe(409);
    await call("DELETE", `/tabs/${tab}`);
  });
});

describe("saved resumes", () => {
  it("lists only ours and deletes the one asked for, confirming in the dialog", async () => {
    const tab = await open("seek-profile");
    const prefixes = ["Test Person - Resume"];
    const list = async () =>
      (await call<{ resumes: { index: number; name: string; added: string }[] }>("POST", `/tabs/${tab}/resumes`, { prefixes }))
        .data.resumes;
    const before = await list();
    expect(before.map((r) => r.name)).not.toContain("My own CV 2023.pdf");
    expect(before.length).toBe(5);
    const oldest = before.find((r) => /20 days/.test(r.added))!;

    const wrong = await call<{ deleted: boolean }>("POST", `/tabs/${tab}/delete-resume`, {
      prefixes,
      index: oldest.index,
      name: "Someone else.pdf",
    });
    expect(wrong.data.deleted).toBe(false);

    const res = await call<{ deleted: boolean }>("POST", `/tabs/${tab}/delete-resume`, {
      prefixes,
      index: oldest.index,
      name: oldest.name,
    });
    expect(res.data.deleted).toBe(true);
    const after = await list();
    expect(after.length).toBe(4);
    expect(after.some((r) => /20 days/.test(r.added))).toBe(false);
    await call("DELETE", `/tabs/${tab}`);
  });
});

describe("saved resumes, SEEK's options menu", () => {
  it("opens the list, then deletes through Options, Delete and the confirmation", async () => {
    const tab = await open("seek-resumes");
    const prefixes = ["Test Person - Resume"];
    const list = async () =>
      (await call<{ resumes: { index: number; name: string; added: string }[] }>("POST", `/tabs/${tab}/resumes`, { prefixes }))
        .data.resumes;
    const before = await list();
    expect(before.map((r) => r.name)).not.toContain("My own CV 2023.pdf");
    expect(before.length).toBe(5);
    expect(before.find((r) => /SEEK-95446655/.test(r.name))!.added).toBe("Added 10 days ago");

    // The default resumé asks "Delete default resumé": that's confirmed too.
    const oldest = before.find((r) => /20 days/.test(r.added))!;
    const res = await call<{ deleted: boolean }>("POST", `/tabs/${tab}/delete-resume`, {
      prefixes,
      index: oldest.index,
      name: oldest.name,
    });
    expect(res.data.deleted).toBe(true);
    const next = (await list()).find((r) => /10 days/.test(r.added))!;
    const again = await call<{ deleted: boolean }>("POST", `/tabs/${tab}/delete-resume`, {
      prefixes,
      index: next.index,
      name: next.name,
    });
    expect(again.data.deleted).toBe(true);
    const after = await list();
    expect(after.length).toBe(3);
    expect(after.some((r) => /(20|10) days/.test(r.added))).toBe(false);
    await call("DELETE", `/tabs/${tab}`);
  });
});

describe("pdf", () => {
  it("prints HTML and counts the pages", async () => {
    const one = await call<{ pdf_base64: string; pages: number }>("POST", "/pdf", { html: "<h1>One page</h1>" });
    expect(one.data.pages).toBe(1);
    expect(Buffer.from(one.data.pdf_base64, "base64").subarray(0, 4).toString()).toBe("%PDF");
    const long = "<p>line</p>".repeat(200);
    const two = await call<{ pages: number }>("POST", "/pdf", { html: long, scale: 1 });
    expect(two.data.pages).toBeGreaterThan(1);
  });
});
