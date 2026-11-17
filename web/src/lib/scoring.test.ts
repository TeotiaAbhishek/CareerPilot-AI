import { describe, expect, it } from "vitest";

import { dollars, isLocalModel, percent, rounded, scoreTone, seconds } from "./scoring";

describe("scoring helpers", () => {
  it("colours scores", () => {
    expect(scoreTone(null, null, null)).toBe("neutral");
    expect(scoreTone(80, 0, true)).toBe("success");
    expect(scoreTone(90, 1, false)).toBe("danger");
    expect(scoreTone(50, 0, false)).toBe("warning");
    expect(scoreTone(20, 0, false)).toBe("danger");
  });

  it("formats numbers", () => {
    expect(percent(5 / 6)).toBe("83%");
    expect(percent(null)).toBe("–");
    expect(rounded(61.4)).toBe("61");
    expect(seconds(12345)).toBe("12.3 s");
  });

  it("tells local models from paid ones", () => {
    expect(isLocalModel("qwen3:14b")).toBe(true);
    expect(isLocalModel("claude-haiku-5-5")).toBe(false);
    expect(isLocalModel("gpt-6-luna")).toBe(false);
  });

  it("formats dollars, keeping small costs readable", () => {
    expect(dollars(null)).toBe("–");
    expect(dollars(0)).toBe("$0");
    expect(dollars(0.00065)).toBe("$0.00065");
    expect(dollars(0.0123)).toBe("$0.01");
    expect(dollars(1.5)).toBe("$1.50");
  });
});
