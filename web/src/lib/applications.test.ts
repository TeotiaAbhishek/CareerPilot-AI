import { describe, expect, it } from "vitest";

import type { Application, ApplicationView } from "./api";
import { applyLiveEvent, isClosed, lastEventId, needsYou, sortForList } from "./applications";

const base: Application = {
  id: "a1",
  job_id: "j1",
  status: "queued",
  manual_reason: null,
  message: null,
  start_url: "https://example.com",
  current_url: null,
  tab_id: null,
  resume_id: null,
  cover_letter_id: null,
  steps: 0,
  practice: false,
  started_at: null,
  ready_at: null,
  submitted_at: null,
  created_at: "2026-11-14T00:00:00Z",
  updated_at: "2026-11-14T00:00:00Z",
  job_title: "Data Analyst",
  company: "Acme",
  open_questions: 0,
};

describe("applications", () => {
  it("lists your turn first, then newest", () => {
    const rows: Application[] = [
      { ...base, id: "old-submitted", status: "submitted", updated_at: "2026-11-14T05:00:00Z" },
      { ...base, id: "queued", status: "queued", updated_at: "2026-11-14T04:00:00Z" },
      { ...base, id: "ready", status: "ready_to_submit", updated_at: "2026-11-14T01:00:00Z" },
      { ...base, id: "question-new", status: "waiting_for_you", updated_at: "2026-11-14T03:00:00Z" },
      { ...base, id: "question-old", status: "waiting_for_you", updated_at: "2026-11-14T02:00:00Z" },
    ];
    expect(sortForList(rows).map((r) => r.id)).toEqual([
      "ready",
      "question-new",
      "question-old",
      "queued",
      "old-submitted",
    ]);
  });

  it("knows whose turn it is", () => {
    expect(needsYou("ready_to_submit")).toBe(true);
    expect(needsYou("needs_manual")).toBe(true);
    expect(needsYou("running")).toBe(false);
    expect(isClosed("cancelled")).toBe(true);
    expect(isClosed("waiting_for_you")).toBe(false);
  });

  it("adds live log lines once and follows status changes", () => {
    const view: ApplicationView = { ...base, questions: [], events: [] };
    const line = { id: 7, application_id: "a1", kind: "fill" as const, message: "Filled", at: "2026-11-14T00:00:01Z" };
    const once = applyLiveEvent(view, { type: "log", ...line });
    const twice = applyLiveEvent(once, { type: "log", ...line });
    expect(twice.events).toEqual([line]);
    expect(lastEventId(twice)).toBe(7);
    expect(applyLiveEvent(twice, { type: "status", application_id: "a1", status: "running" }).status).toBe("running");
  });
});
