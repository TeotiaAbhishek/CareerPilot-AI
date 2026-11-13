/**
 * The one place the web app talks to the Rust API.
 * Every call goes through `apiGet` / `apiSend`, so errors look the same everywhere.
 */

export const API_URL = process.env.NEXT_PUBLIC_API_URL ?? "http://localhost:8080";

export class ApiError extends Error {
  constructor(
    message: string,
    public readonly status: number,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let response: Response;

  try {
    response = await fetch(`${API_URL}${path}`, {
      ...init,
      headers: { Accept: "application/json", ...init?.headers },
    });
  } catch {
    // Network error: the API isn't running or isn't reachable.
    throw new ApiError("Can't reach the CareerPilot API.", 0);
  }

  if (!response.ok) {
    // The API always answers errors as {"error": "..."}.
    const body = (await response.json().catch(() => null)) as { error?: string } | null;
    throw new ApiError(body?.error ?? `Request failed (${response.status}).`, response.status);
  }

  // 204 No Content has no body.
  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

export function apiGet<T>(path: string, init?: RequestInit): Promise<T> {
  return request<T>(path, init);
}

export function apiSend<T>(
  method: "POST" | "PUT" | "PATCH" | "DELETE",
  path: string,
  body?: unknown,
): Promise<T> {
  return request<T>(path, {
    method,
    headers: body === undefined ? undefined : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

/** A readable message from anything thrown. */
export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : "Something went wrong.";
}

// ---------- Types (mirror the Rust structs) ----------

export type Health = {
  status: "ok";
  database: "ok" | "unreachable";
  version: string;
};

export type JobSummary = {
  id: string;
  title: string;
  company: string | null;
  location: string | null;
  work_type: string | null;
  salary: string | null;
  short_description: string | null;
  posted_label: string | null;
  listed_at: string | null;
  first_seen_at: string;
  apply_type: string | null;
  saved: boolean;
  hidden: boolean;
  viewed_at: string | null;
  scrape_status: "ok" | "failed";
};

export type JobDetail = JobSummary & {
  classification: string | null;
  subclassification: string | null;
  description_html: string | null;
  description_text: string | null;
  job_url: string;
  apply_url: string | null;
  last_seen_at: string;
  scrape_error: string | null;
  legacy_score: number | null;
};

export type JobPage = {
  items: JobSummary[];
  total: number;
  page: number;
  page_size: number;
};

export type Facet = { value: string; count: number };

export type Stats = {
  jobs_total: number;
  jobs_new_24h: number;
  jobs_unviewed: number;
  jobs_saved: number;
};

export type Search = {
  id: string;
  keywords: string;
  location: string;
  max_pages: number;
  enabled: boolean;
  created_at: string;
  updated_at: string;
};

export type SearchInput = Pick<Search, "keywords" | "location" | "max_pages" | "enabled">;

export type ScrapeRun = {
  id: string;
  trigger: "manual" | "schedule";
  status: "running" | "succeeded" | "failed" | "rate_limited";
  message: string | null;
  pages_fetched: number;
  jobs_found: number;
  jobs_new: number;
  jobs_failed: number;
  started_at: string;
  finished_at: string | null;
};

export type ScrapeStatus = {
  current: ScrapeRun | null;
  recent: ScrapeRun[];
  schedule: { daily_at: string | null; timezone: string; next_run_at: string | null };
};

export type JobPatch = { saved?: boolean; hidden?: boolean; viewed?: boolean };

// ---------- Endpoints ----------

export const getHealth = (init?: RequestInit) => apiGet<Health>("/api/health", init);
export const patchJob = (id: string, patch: JobPatch) =>
  apiSend<JobDetail>("PATCH", `/api/jobs/${encodeURIComponent(id)}`, patch);
export const startScrape = () => apiSend<ScrapeRun>("POST", "/api/scrape");
export const getScrapeStatus = () => apiGet<ScrapeStatus>("/api/scrape/status", { cache: "no-store" });
export const createSearch = (input: SearchInput) => apiSend<Search>("POST", "/api/searches", input);
export const updateSearch = (id: string, input: SearchInput) =>
  apiSend<Search>("PUT", `/api/searches/${id}`, input);
export const deleteSearch = (id: string) => apiSend<void>("DELETE", `/api/searches/${id}`);
