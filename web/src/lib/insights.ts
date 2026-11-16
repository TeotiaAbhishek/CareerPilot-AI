import type { Breakdown, Insights } from "./api";

/** 0.4567 -> "46%". null -> "–". */
export const rate = (value: number | null) => (value === null ? "–" : `${Math.round(value * 100)}%`);

/** Replied out of applied for one group, or null when nothing was applied. */
export const replyRate = (b: Breakdown) => (b.applied === 0 ? null : b.replied / b.applied);

/** The funnel as bars, each with its share of what you applied to. */
export function funnelSteps(funnel: Insights["funnel"]) {
  const steps = [
    { label: "Applied", count: funnel.applied },
    { label: "Replied", count: funnel.replied },
    { label: "Interview", count: funnel.interview },
    { label: "Offer", count: funnel.offer },
  ];
  return steps.map((s) => ({ ...s, share: funnel.applied === 0 ? 0 : s.count / funnel.applied }));
}

/** "2026-11-16" -> "16 Nov". Read as a calendar day, not a UTC time. */
export function dayLabel(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString("en-AU", { day: "numeric", month: "short" });
}

/** "4.5" -> "4.5 days", "1" -> "1 day", null -> "–". */
export function days(value: number | null): string {
  if (value === null) return "–";
  const rounded = Math.round(value * 10) / 10;
  return `${rounded} day${rounded === 1 ? "" : "s"}`;
}
