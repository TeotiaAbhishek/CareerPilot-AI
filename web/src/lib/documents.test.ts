import { describe, expect, it } from "vitest";

import type { Flag } from "./api";
import { flagsAt, inlineMarkdown, move, openErrors, wordCount } from "./documents";

const flags: Flag[] = [
  { key: "term|summary|AWS", path: "summary", severity: "error", message: "AWS" },
  { key: "number|summary|35", path: "summary", severity: "error", message: "35" },
  { key: "note|document|0", path: "document", severity: "info", message: "note" },
];

describe("flags", () => {
  it("splits a line's flags into open and dismissed", () => {
    const { open, dismissed } = flagsAt(flags, "summary", ["term|summary|AWS"]);
    expect(open.map((f) => f.message)).toEqual(["35"]);
    expect(dismissed.map((f) => f.message)).toEqual(["AWS"]);
  });

  it("counts only open errors", () => {
    expect(openErrors(flags, [])).toBe(2);
    expect(openErrors(flags, ["term|summary|AWS", "number|summary|35"])).toBe(0);
  });
});

describe("inlineMarkdown", () => {
  it("handles bold and links", () => {
    expect(inlineMarkdown("**Analyst** | [LinkedIn](https://www.linkedin.com/in/x/)")).toEqual([
      { text: "Analyst", bold: true },
      { text: " | " },
      { text: "linkedin.com/in/x", href: "https://www.linkedin.com/in/x/" },
    ]);
  });

  it("leaves plain text alone", () => {
    expect(inlineMarkdown("Sydney, NSW")).toEqual([{ text: "Sydney, NSW" }]);
  });
});

describe("helpers", () => {
  it("counts words", () => {
    expect(wordCount([{ text: " one two ", evidence: [] }, { text: "three", evidence: [] }])).toBe(3);
  });

  it("moves items and ignores moves off the end", () => {
    expect(move(["a", "b", "c"], 0, 1)).toEqual(["b", "a", "c"]);
    expect(move(["a", "b"], 0, -1)).toEqual(["a", "b"]);
  });
});
