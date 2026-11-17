/**
 * Small helpers for showing scores. Kept free of React so they're easy to test.
 */

import type { JobLabel } from "./api";

export type ScoreTone = "success" | "warning" | "danger" | "neutral";

/**
 * Green when relevant, amber when close (within 15 of the threshold),
 * red when blocked or far below. Grey when there's no score yet.
 */
export function scoreTone(
  score: number | null,
  blockers: number | null,
  relevant: boolean | null,
): ScoreTone {
  if (score === null) return "neutral";
  if (relevant) return "success";
  if ((blockers ?? 0) > 0) return "danger";
  return score >= 45 ? "warning" : "danger";
}

/** 0.8333 -> "83%". null -> "–". */
export function percent(value: number | null): string {
  return value === null ? "–" : `${Math.round(value * 100)}%`;
}

/** 61.25 -> "61". null -> "–". */
export function rounded(value: number | null): string {
  return value === null ? "–" : String(Math.round(value));
}

/** True for a model that runs on your computer (Ollama), not Claude or OpenAI. */
export function isLocalModel(model: string): boolean {
  return !model.startsWith("claude-") && !model.startsWith("gpt-");
}

/** US dollars: 0.00065 -> "$0.00065", 1.5 -> "$1.50", null -> "–". */
export function dollars(value: number | null): string {
  if (value === null) return "–";
  if (value === 0) return "$0";
  if (value < 0.01) return `$${Number(value.toPrecision(2))}`;
  return `$${value.toFixed(2)}`;
}

/** 12345 -> "12.3 s". */
export function seconds(ms: number): string {
  return `${(ms / 1000).toFixed(1)} s`;
}

export const LABELS: { value: JobLabel; text: string }[] = [
  { value: "apply", text: "Apply" },
  { value: "maybe", text: "Maybe" },
  { value: "no", text: "No" },
];

export const LABEL_TONE: Record<JobLabel, "success" | "warning" | "danger"> = {
  apply: "success",
  maybe: "warning",
  no: "danger",
};
