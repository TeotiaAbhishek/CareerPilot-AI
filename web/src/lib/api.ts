/**
 * The one place the web app talks to the Rust API.
 * Every call goes through `apiGet`, so errors look the same everywhere.
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

export async function apiGet<T>(path: string, init?: RequestInit): Promise<T> {
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

  return (await response.json()) as T;
}

export type Health = {
  status: "ok";
  database: "ok" | "unreachable";
  version: string;
};

export const getHealth = (init?: RequestInit) => apiGet<Health>("/api/health", init);
