import type { Card, Column, OneClick, Stage } from "./api";

type Tone = "neutral" | "primary" | "success" | "warning" | "danger";

/** The board, left to right. `stage` is set on the columns you can move a card to. */
export const COLUMNS: { id: Column; label: string; tone: Tone; stage?: Stage }[] = [
  { id: "ready", label: "Ready to apply", tone: "neutral" },
  { id: "applying", label: "Applying", tone: "primary" },
  { id: "your_turn", label: "Your turn", tone: "warning" },
  { id: "submitted", label: "Submitted", tone: "primary", stage: "submitted" },
  { id: "interview", label: "Interview", tone: "success", stage: "interview" },
  { id: "offer", label: "Offer", tone: "success", stage: "offer" },
  { id: "rejected", label: "Rejected", tone: "danger", stage: "rejected" },
  { id: "withdrawn", label: "Withdrawn", tone: "neutral", stage: "withdrawn" },
];

export const columnInfo = (id: Column) => COLUMNS.find((c) => c.id === id)!;

/** The stage a card gets when you drop it on `column`, or null if you can't. */
export function dropStage(card: Card, column: Column): Stage | null {
  const stage = columnInfo(column).stage;
  return stage && card.column !== column ? stage : null;
}

/** Where a card can go from its card menu. */
export const moveTargets = (card: Card) => COLUMNS.filter((c) => c.stage && c.id !== card.column);

/** Cards grouped by column, every column present (maybe empty). */
export function groupByColumn(cards: Card[]): Record<Column, Card[]> {
  const groups = Object.fromEntries(COLUMNS.map((c) => [c.id, [] as Card[]])) as Record<Column, Card[]>;
  for (const card of cards) groups[card.column].push(card);
  return groups;
}

export type ListFilters = { column: Column | "all"; company: string; days: number | null };

export const NO_FILTERS: ListFilters = { column: "all", company: "", days: null };

/**
 * The list view's filters: a column, part of a company name, and "changed in
 * the last N days".
 */
export function filterCards(cards: Card[], f: ListFilters, now: Date = new Date()): Card[] {
  const company = f.company.trim().toLowerCase();
  const since = f.days === null ? null : now.getTime() - f.days * 86_400_000;
  return cards.filter(
    (c) =>
      (f.column === "all" || c.column === f.column) &&
      (!company || (c.company ?? "").toLowerCase().includes(company)) &&
      (since === null || new Date(c.updated_at).getTime() >= since),
  );
}

/** The toast after one-click Apply. */
export const STARTED: Record<OneClick["started"], string> = {
  applying: "The agent is applying. It stops before Submit.",
  drafting: "Drafting your resume. The agent starts once it's approved.",
  waiting_for_you: "Your resume needs a look first. Approve it and the agent starts.",
};

/** "3 started. 1 skipped: Already applied." */
export function bulkSummary(started: number, skipped: { reason: string }[]): string {
  const head = `${started} started.`;
  if (skipped.length === 0) return head;
  const reasons = Array.from(new Set(skipped.map((s) => s.reason))).join(" ");
  return `${head} ${skipped.length} skipped: ${reasons}`;
}
