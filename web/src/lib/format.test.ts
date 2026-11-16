import { describe, expect, it } from "vitest";

import { formatDate, timeAgo, timeUntil } from "./format";

const now = new Date("2026-11-14T12:00:00Z");
const ago = (ms: number) => new Date(now.getTime() - ms).toISOString();

describe("timeAgo", () => {
  it("formats recent times", () => {
    expect(timeAgo(ago(10_000), now)).toBe("just now");
    expect(timeAgo(ago(5 * 60_000), now)).toBe("5 min ago");
    expect(timeAgo(ago(3 * 3_600_000), now)).toBe("3 h ago");
    expect(timeAgo(ago(26 * 3_600_000), now)).toBe("yesterday");
    expect(timeAgo(ago(5 * 86_400_000), now)).toBe("5 days ago");
  });

  it("returns empty text for missing dates", () => {
    expect(timeAgo(null, now)).toBe("");
  });
});

describe("timeUntil", () => {
  it("formats future times", () => {
    expect(timeUntil(new Date(now.getTime() + 25 * 60_000).toISOString(), now)).toBe("in 25 min");
    expect(timeUntil(new Date(now.getTime() + 9 * 3_600_000).toISOString(), now)).toBe("in 9 h");
  });
});

describe("formatDate", () => {
  const now = new Date("2026-11-16T12:00:00");

  it("leaves the year out for this year", () => {
    expect(formatDate("2026-03-05T12:00:00", now)).toBe("5 Mar");
    expect(formatDate("2025-03-05T12:00:00", now)).toBe("5 Mar 2025");
    expect(formatDate(null, now)).toBe("");
  });
});
