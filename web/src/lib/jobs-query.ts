/**
 * The Jobs page keeps its filters in the URL (/jobs?q=sql&status=saved),
 * so a filtered view can be bookmarked, shared and survives a refresh.
 * These helpers convert between the URL and a typed object.
 */

export type JobStatus = "all" | "new" | "saved" | "hidden";
export type JobSort = "newest" | "oldest" | "relevance";

export type JobFilters = {
  q: string;
  status: JobStatus;
  workType: string;
  postedWithinDays: number | null;
  sort: JobSort;
  page: number;
};

export const DEFAULT_FILTERS: JobFilters = {
  q: "",
  status: "all",
  workType: "",
  postedWithinDays: null,
  sort: "newest",
  page: 1,
};

export const PAGE_SIZE = 25;

const STATUSES: JobStatus[] = ["all", "new", "saved", "hidden"];
const SORTS: JobSort[] = ["newest", "oldest", "relevance"];

type ParamsLike = { get(name: string): string | null };

export function parseFilters(params: ParamsLike): JobFilters {
  const status = params.get("status") as JobStatus | null;
  const sort = params.get("sort") as JobSort | null;
  const days = Number(params.get("posted"));
  const page = Number(params.get("page"));

  return {
    q: params.get("q") ?? "",
    status: status && STATUSES.includes(status) ? status : "all",
    workType: params.get("work_type") ?? "",
    postedWithinDays: Number.isInteger(days) && days > 0 ? days : null,
    sort: sort && SORTS.includes(sort) ? sort : "newest",
    page: Number.isInteger(page) && page > 1 ? page : 1,
  };
}

/** The page URL's query string. Defaults are left out to keep URLs short. */
export function filtersToSearchParams(filters: JobFilters, selectedJob?: string | null): string {
  const params = new URLSearchParams();
  if (filters.q) params.set("q", filters.q);
  if (filters.status !== "all") params.set("status", filters.status);
  if (filters.workType) params.set("work_type", filters.workType);
  if (filters.postedWithinDays) params.set("posted", String(filters.postedWithinDays));
  if (filters.sort !== "newest") params.set("sort", filters.sort);
  if (filters.page > 1) params.set("page", String(filters.page));
  if (selectedJob) params.set("job", selectedJob);
  return params.toString();
}

/** The API path for these filters: /api/jobs?... */
export function filtersToApiPath(filters: JobFilters): string {
  const params = new URLSearchParams({
    status: filters.status,
    sort: filters.sort === "relevance" && !filters.q ? "newest" : filters.sort,
    page: String(filters.page),
    page_size: String(PAGE_SIZE),
  });
  if (filters.q.trim()) params.set("q", filters.q.trim());
  if (filters.workType) params.set("work_type", filters.workType);
  if (filters.postedWithinDays) params.set("posted_within_days", String(filters.postedWithinDays));
  return `/api/jobs?${params.toString()}`;
}
