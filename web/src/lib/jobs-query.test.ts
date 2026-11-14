import { describe, expect, it } from "vitest";

import {
  DEFAULT_FILTERS,
  filtersToApiPath,
  filtersToSearchParams,
  parseFilters,
} from "./jobs-query";

describe("jobs query helpers", () => {
  it("reads defaults from an empty URL", () => {
    expect(parseFilters(new URLSearchParams())).toEqual(DEFAULT_FILTERS);
  });

  it("ignores junk values instead of breaking", () => {
    const f = parseFilters(new URLSearchParams("status=weird&sort=up&page=-3&posted=abc"));
    expect(f.status).toBe("all");
    expect(f.sort).toBe("newest");
    expect(f.page).toBe(1);
    expect(f.postedWithinDays).toBeNull();
  });

  it("round-trips through the URL", () => {
    const filters = {
      ...DEFAULT_FILTERS,
      q: "sql",
      status: "saved" as const,
      page: 2,
      postedWithinDays: 7,
      relevantOnly: false,
      sort: "best_fit" as const,
    };
    const url = filtersToSearchParams(filters, "123");
    expect(url).toBe("q=sql&status=saved&posted=7&relevant=0&sort=best_fit&page=2&job=123");
    expect(parseFilters(new URLSearchParams(url))).toEqual(filters);
  });

  it("builds the API path, falling back from relevance when there's no search text", () => {
    const path = filtersToApiPath({ ...DEFAULT_FILTERS, relevantOnly: false, sort: "relevance" });
    expect(path).toContain("sort=newest");
    expect(path).not.toContain("q=");
    expect(path).not.toContain("relevant_only");
  });

  it("shows relevant jobs only unless the URL says relevant=0", () => {
    expect(parseFilters(new URLSearchParams()).relevantOnly).toBe(true);
    expect(parseFilters(new URLSearchParams("relevant=0")).relevantOnly).toBe(false);
    expect(filtersToSearchParams(DEFAULT_FILTERS)).toBe("");
  });

  it("sends the relevant filter and best-fit sort to the API", () => {
    const path = filtersToApiPath({ ...DEFAULT_FILTERS, relevantOnly: true, sort: "best_fit" });
    expect(path).toContain("relevant_only=true");
    expect(path).toContain("sort=best_fit");
  });
});
