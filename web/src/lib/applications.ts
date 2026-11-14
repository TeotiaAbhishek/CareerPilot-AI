import type { Application, ApplicationStatus, ApplicationView, LiveEvent, ManualReason, RunEventKind } from "./api";

type Tone = "neutral" | "primary" | "success" | "warning" | "danger";

export const STATUS: Record<ApplicationStatus, { label: string; tone: Tone }> = {
  queued: { label: "Queued", tone: "neutral" },
  running: { label: "Filling in", tone: "primary" },
  waiting_for_you: { label: "Question for you", tone: "warning" },
  ready_to_submit: { label: "Ready to submit", tone: "success" },
  needs_manual: { label: "Needs you", tone: "warning" },
  submitted: { label: "Submitted", tone: "success" },
  failed: { label: "Failed", tone: "danger" },
  cancelled: { label: "Cancelled", tone: "neutral" },
};

export const MANUAL_REASON: Record<ManualReason, string> = {
  captcha: "CAPTCHA",
  login: "Login needed",
  stuck: "Form it couldn't finish",
  cover_letter: "Needs a cover letter",
  step_limit: "Too many steps",
};

export const EVENT_TONE: Record<RunEventKind, string> = {
  info: "text-foreground",
  fill: "text-muted-foreground",
  model: "text-primary",
  written: "text-primary font-medium",
  answer: "text-warning",
  click: "text-muted-foreground",
  warn: "text-warning",
  error: "text-danger",
};

/** The agent is still working on it, or will be soon. */
export const isActive = (status: ApplicationStatus) => status === "queued" || status === "running";

/** Your turn: answer, submit, or finish by hand. */
export const needsYou = (status: ApplicationStatus) =>
  status === "waiting_for_you" || status === "ready_to_submit" || status === "needs_manual";

/** Finished one way or another: Apply can start a new run. */
export const isClosed = (status: ApplicationStatus) =>
  status === "submitted" || status === "cancelled" || status === "failed";

const ORDER: ApplicationStatus[] = [
  "ready_to_submit",
  "waiting_for_you",
  "needs_manual",
  "running",
  "queued",
  "failed",
  "submitted",
  "cancelled",
];

/** Your turn first, then what's running, then the rest; newest first within each. */
export function sortForList(rows: Application[]): Application[] {
  return [...rows].sort(
    (a, b) => ORDER.indexOf(a.status) - ORDER.indexOf(b.status) || b.updated_at.localeCompare(a.updated_at),
  );
}

/** Apply one live event to the run view: add a log line or change the status. */
export function applyLiveEvent(view: ApplicationView, event: LiveEvent): ApplicationView {
  if (event.type === "status") {
    return { ...view, status: event.status };
  }
  if (view.events.some((e) => e.id === event.id)) return view;
  const line = { id: event.id, application_id: event.application_id, kind: event.kind, message: event.message, at: event.at };
  return { ...view, events: [...view.events, line] };
}

/** The id of the newest log line, so a reconnect only asks for what's new. */
export const lastEventId = (view: ApplicationView) => view.events.at(-1)?.id ?? 0;
