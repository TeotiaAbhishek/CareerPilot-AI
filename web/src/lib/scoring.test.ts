import { describe, expect, it } from "vitest";

import { percent, rounded, scoreTone, seconds } from "./scoring";

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
});
