import { describe, expect, it } from "vitest";

import { dayLabel, days, funnelSteps, rate, replyRate } from "./insights";

describe("insights helpers", () => {
  it("formats rates and days", () => {
    expect(rate(0.4567)).toBe("46%");
    expect(rate(null)).toBe("–");
    expect(days(1)).toBe("1 day");
    expect(days(4.46)).toBe("4.5 days");
    expect(days(null)).toBe("–");
  });

  it("reads a day as a calendar date", () => {
    expect(dayLabel("2026-11-16")).toBe("16 Nov");
  });

  it("works out reply rates and funnel shares", () => {
    expect(replyRate({ label: "x", applied: 4, replied: 1, interviews: 0 })).toBe(0.25);
    expect(replyRate({ label: "x", applied: 0, replied: 0, interviews: 0 })).toBeNull();
    const steps = funnelSteps({ applied: 10, replied: 4, interview: 2, offer: 1, rejected: 3 });
    expect(steps.map((s) => s.share)).toEqual([1, 0.4, 0.2, 0.1]);
    expect(funnelSteps({ applied: 0, replied: 0, interview: 0, offer: 0, rejected: 0 })[0].share).toBe(0);
  });
});
