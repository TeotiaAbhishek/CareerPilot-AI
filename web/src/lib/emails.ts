import type { Email, EmailKind, EmailStatus } from "./api";

type Tone = "neutral" | "primary" | "success" | "warning" | "danger";

export const KIND_LABEL: Record<EmailKind, string> = {
  application: "Application email",
  follow_up_1: "Follow-up",
  follow_up_2: "Second follow-up",
  thank_you: "Thank-you",
};

export const EMAIL_STATUS: Record<EmailStatus, { label: string; tone: Tone }> = {
  scheduled: { label: "Scheduled", tone: "neutral" },
  queued: { label: "Queued", tone: "neutral" },
  drafting: { label: "Drafting", tone: "primary" },
  ready: { label: "Ready to send", tone: "success" },
  sent: { label: "Sent", tone: "neutral" },
  cancelled: { label: "Skipped", tone: "neutral" },
  failed: { label: "Failed", tone: "danger" },
};

/** The model is still writing it (or will be soon). */
export const isDrafting = (status: EmailStatus) => status === "queued" || status === "drafting";

/** The whole email as plain text: greeting, paragraphs, sign-off. */
export function plainText(email: Pick<Email, "paragraphs">, greeting: string, signOff: string): string {
  const body = email.paragraphs.map((p) => p.text.trim()).filter(Boolean);
  return [greeting, ...body, signOff].join("\n\n");
}

/**
 * A Gmail "new message" link with everything filled in. Gmail opens the
 * compose window in your browser; you check it and click Send.
 */
export function gmailUrl(to: string | null, subject: string, body: string): string {
  const params = new URLSearchParams({ view: "cm", fs: "1", su: subject, body });
  if (to) params.set("to", to);
  return `https://mail.google.com/mail/?${params.toString()}`;
}

/** Words in the paragraphs (not counting the greeting and sign-off). */
export const wordCount = (email: Pick<Email, "paragraphs">) =>
  email.paragraphs.reduce((n, p) => n + p.text.split(/\s+/).filter(Boolean).length, 0);
