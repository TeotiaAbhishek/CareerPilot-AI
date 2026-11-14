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
