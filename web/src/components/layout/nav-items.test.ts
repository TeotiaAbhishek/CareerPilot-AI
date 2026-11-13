import { describe, expect, it } from "vitest";

import { isActive } from "./nav-items";

describe("isActive", () => {
  it("matches the dashboard only on /", () => {
    expect(isActive("/", "/")).toBe(true);
    expect(isActive("/jobs", "/")).toBe(false);
  });

  it("matches a section and its sub-pages", () => {
    expect(isActive("/jobs", "/jobs")).toBe(true);
    expect(isActive("/jobs/123", "/jobs")).toBe(true);
  });

  it("does not match a different section with the same prefix", () => {
    expect(isActive("/jobsearch", "/jobs")).toBe(false);
  });
});
