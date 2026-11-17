import type { DocumentKind, DocumentStatus, DraftLine, Flag } from "./api";

type Tone = "neutral" | "primary" | "success" | "warning" | "danger";

export const KIND_LABEL: Record<DocumentKind, string> = {
  resume: "Resume",
  cover_letter: "Cover letter",
};

export const STATUS: Record<DocumentStatus, { label: string; tone: Tone }> = {
  queued: { label: "Queued", tone: "neutral" },
  drafting: { label: "Drafting", tone: "primary" },
  ready: { label: "Needs review", tone: "warning" },
  approved: { label: "Approved", tone: "success" },
  failed: { label: "Failed", tone: "danger" },
};

/** True while the worker still has to do something with it. */
export const isPending = (status: DocumentStatus | undefined) =>
  status === "queued" || status === "drafting";

/** The flags for one line, split into ones to act on and ones you dismissed. */
export function flagsAt(flags: Flag[], path: string, dismissed: string[]) {
  const here = flags.filter((f) => f.path === path);
  return {
    open: here.filter((f) => !dismissed.includes(f.key)),
    dismissed: here.filter((f) => dismissed.includes(f.key)),
  };
}

/** Error flags you haven't dismissed. Approval needs 0. */
export function openErrors(flags: Flag[], dismissed: string[]): number {
  return flags.filter((f) => f.severity === "error" && !dismissed.includes(f.key)).length;
}

export function wordCount(lines: DraftLine[]): number {
  return lines.reduce((n, l) => n + l.text.split(/\s+/).filter(Boolean).length, 0);
}

/** A copy of `items` with one item moved (out-of-range moves do nothing). */
export function move<T>(items: T[], from: number, to: number): T[] {
  if (to < 0 || to >= items.length || from === to) return items;
  const copy = [...items];
  const [item] = copy.splice(from, 1);
  copy.splice(to, 0, item);
  return copy;
}

export type Segment = { text: string; bold?: boolean; href?: string };

/**
 * The small bit of Markdown resumes use: **bold** and [label](url).
 * Links show their URL without "https://www." (that's what gets printed).
 */
export function inlineMarkdown(text: string): Segment[] {
  const out: Segment[] = [];
  const pattern = /\*\*(.+?)\*\*|\[([^\]]+)\]\(([^)\s]+)\)/g;
  let last = 0;
  for (const match of text.matchAll(pattern)) {
    const at = match.index ?? 0;
    if (at > last) out.push({ text: text.slice(last, at) });
    if (match[1] !== undefined) {
      out.push({ text: match[1], bold: true });
    } else {
      const href = match[3];
      out.push({ text: href.replace(/^https?:\/\/(www\.)?/, "").replace(/\/$/, ""), href });
    }
    last = at + match[0].length;
  }
  if (last < text.length) out.push({ text: text.slice(last) });
  return out;
}

/** Bullets split at each new subheading (a project inside a job), keeping each bullet's index. */
export function bulletGroups(lines: DraftLine[]): { group?: string; items: { line: DraftLine; index: number }[] }[] {
  const out: { group?: string; items: { line: DraftLine; index: number }[] }[] = [];
  lines.forEach((line, index) => {
    const last = out[out.length - 1];
    if (!last || (line.group && line.group !== last.group)) {
      out.push({ group: line.group, items: [{ line, index }] });
    } else {
      last.items.push({ line, index });
    }
  });
  return out;
}
