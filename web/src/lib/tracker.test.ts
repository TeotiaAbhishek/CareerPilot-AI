import { describe, expect, it } from "vitest";

import type { Card } from "./api";
import { bulkSummary, dropStage, filterCards, groupByColumn, moveTargets, NO_FILTERS } from "./tracker";

const card = (patch: Partial<Card>): Card => ({
  job_id: "j1",
  title: "Data Analyst",
  company: "Acme",
  location: null,
  apply_type: null,
  score: null,
  column: "ready",
  detail: "",
  application_id: null,
  emails_due: 0,
  applied_at: null,
  replied_at: null,
  updated_at: "2026-11-16T00:00:00Z",
  created_at: "2026-11-16T00:00:00Z",
  ...patch,
});

describe("dropStage", () => {
  it("only lets you drop on the columns you set", () => {
    expect(dropStage(card({}), "submitted")).toBe("submitted");
    expect(dropStage(card({ column: "submitted" }), "interview")).toBe("interview");
    expect(dropStage(card({}), "applying")).toBeNull();
    expect(dropStage(card({ column: "offer" }), "offer")).toBeNull();
  });
});

describe("moveTargets", () => {
  it("lists the other after-Submit columns", () => {
    expect(moveTargets(card({ column: "submitted" })).map((c) => c.id)).toEqual([
      "interview",
      "offer",
      "rejected",
      "withdrawn",
    ]);
  });
});

describe("groupByColumn", () => {
  it("keeps every column, even empty ones", () => {
    const groups = groupByColumn([card({ column: "interview" })]);
    expect(groups.interview).toHaveLength(1);
    expect(groups.ready).toEqual([]);
  });
});

describe("filterCards", () => {
  const now = new Date("2026-11-16T12:00:00Z");
  const cards = [
    card({ job_id: "a", company: "Acme Bank", column: "submitted" }),
    card({ job_id: "b", company: "Globex", updated_at: "2026-10-01T00:00:00Z" }),
  ];

  it("filters by column, company and date", () => {
    expect(filterCards(cards, NO_FILTERS, now)).toHaveLength(2);
    expect(filterCards(cards, { ...NO_FILTERS, column: "submitted" }, now).map((c) => c.job_id)).toEqual(["a"]);
    expect(filterCards(cards, { ...NO_FILTERS, company: " acme " }, now).map((c) => c.job_id)).toEqual(["a"]);
    expect(filterCards(cards, { ...NO_FILTERS, days: 7 }, now).map((c) => c.job_id)).toEqual(["a"]);
  });
});

describe("bulkSummary", () => {
  it("says how many started and why the rest didn't", () => {
    expect(bulkSummary(2, [])).toBe("2 started.");
    expect(bulkSummary(1, [{ reason: "Already applied." }, { reason: "Already applied." }])).toBe(
      "1 started. 2 skipped: Already applied.",
    );
  });
});
