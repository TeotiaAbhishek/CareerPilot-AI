import { describe, expect, it } from "vitest";

import { gmailUrl, plainText, wordCount } from "./emails";

const email = {
  paragraphs: [
    { text: " I built a fraud model. ", evidence: ["W1.1"] },
    { text: "", evidence: [] },
    { text: "Happy to talk.", evidence: ["W1"] },
  ],
};

describe("emails", () => {
  it("puts the greeting, paragraphs and sign-off together", () => {
    expect(plainText(email, "Dear Hiring Manager,", "Kind regards,\nJane")).toBe(
      "Dear Hiring Manager,\n\nI built a fraud model.\n\nHappy to talk.\n\nKind regards,\nJane",
    );
  });

  it("counts the words you wrote", () => {
    expect(wordCount(email)).toBe(8);
  });

  it("builds a Gmail compose link, encoding everything", () => {
    const url = new URL(gmailUrl("jobs@acme.com", "Data Analyst & more", "Hi,\n\nThanks"));
    expect(url.origin + url.pathname).toBe("https://mail.google.com/mail/");
    expect(url.searchParams.get("view")).toBe("cm");
    expect(url.searchParams.get("to")).toBe("jobs@acme.com");
    expect(url.searchParams.get("su")).toBe("Data Analyst & more");
    expect(url.searchParams.get("body")).toBe("Hi,\n\nThanks");
    expect(new URL(gmailUrl(null, "x", "y")).searchParams.has("to")).toBe(false);
  });
});
