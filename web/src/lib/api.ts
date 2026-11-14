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

/** Fields every job has, in the list and in the detail. */
type JobBase = {
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
  /** Your label: the ground truth for evals. */
  label: JobLabel | null;
  /** score >= threshold and no hard blockers. null until scored. */
  relevant: boolean | null;
};

export type JobLabel = "apply" | "maybe" | "no";

export type JobSummary = JobBase & {
  /** Latest score from the active model, null until scored. */
  score: number | null;
  blocker_count: number | null;
};

export type JobScore = {
  model: string;
  prompt_version: string;
  score: number;
  summary: string;
  matched: string[];
  missing: string[];
  unknown: string[];
  hard_blockers: string[];
  duration_ms: number;
  created_at: string;
};

export type JobDetail = JobBase & {
  classification: string | null;
  subclassification: string | null;
  description_html: string | null;
  description_text: string | null;
  job_url: string;
  apply_url: string | null;
  last_seen_at: string;
  scrape_error: string | null;
  legacy_score: number | null;
  score: JobScore | null;
  score_error: string | null;
  /** A Re-score is waiting or running. */
  score_pending: boolean;
};

/** The list fields that change when you act on a job in the detail pane. */
export function summaryPatch(job: JobDetail): Partial<JobSummary> {
  return {
    saved: job.saved,
    hidden: job.hidden,
    viewed_at: job.viewed_at,
    label: job.label,
    relevant: job.relevant,
    score: job.score?.score ?? null,
    blocker_count: job.score?.hard_blockers.length ?? null,
  };
}

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
  jobs_scored: number;
  jobs_relevant: number;
};

export type Settings = {
  scoring_enabled: boolean;
  scoring_model: string;
  relevance_threshold: number;
  updated_at: string;
};

export type SettingsPatch = Partial<
  Pick<Settings, "scoring_enabled" | "scoring_model" | "relevance_threshold">
>;

export type ScoringStatus = {
  enabled: boolean;
  model: string;
  prompt_version: string;
  threshold: number;
  ollama: {
    url: string;
    reachable: boolean;
    models: string[];
    model_installed: boolean;
    error: string | null;
  };
  queue: { requested: number; unscored: number; failed_recently: number };
  worker: {
    current: { job_id: string; title: string; model: string; started_at: string } | null;
    paused_reason: string | null;
    last_scored_at: string | null;
  };
};

export type EvalMetrics = {
  scored: number;
  true_positive: number;
  false_positive: number;
  false_negative: number;
  true_negative: number;
  agreement: number | null;
  precision: number | null;
  recall: number | null;
  mean_score_apply: number | null;
  mean_score_maybe: number | null;
  mean_score_no: number | null;
};

export type EvalRow = {
  source: "ollama" | "v1";
  model: string;
  prompt_version: string;
  active: boolean;
  metrics: EvalMetrics;
};

export type Disagreement = {
  job_id: string;
  title: string;
  company: string | null;
  label: JobLabel;
  score: number;
  blockers: number;
  summary: string;
  v1_score: number | null;
};

export type EvalReport = {
  threshold: number;
  active_model: string;
  prompt_version: string;
  labels: { apply: number; maybe: number; no: number; total: number };
  rows: EvalRow[];
  disagreements: Disagreement[];
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
export const setJobLabel = (id: string, label: JobLabel | null) =>
  apiSend<{ label: JobLabel | null }>("PUT", `/api/jobs/${encodeURIComponent(id)}/label`, { label });
export const rescoreJob = (id: string) =>
  apiSend<void>("POST", `/api/jobs/${encodeURIComponent(id)}/rescore`);
export const updateSettings = (patch: SettingsPatch) => apiSend<Settings>("PUT", "/api/settings", patch);
export const runEval = (model: string) => apiSend<{ queued: number }>("POST", "/api/evals/run", { model });
