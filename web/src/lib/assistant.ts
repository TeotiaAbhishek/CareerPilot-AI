import type { AskEvent, ChatMessage, Chip } from "@/lib/api";

/** Shown on an empty chat. */
export const STARTERS = [
  "Which jobs should I apply to first?",
  "What skill keeps coming up missing?",
  "How are my applications going?",
  "Any new jobs this week that fit me well?",
];

/** The answer being written right now. */
export type Draft = { text: string; chips: Chip[] };

/** What this page added to a chat since it loaded it. */
export type Live = { chatId: string | null; messages: ChatMessage[]; draft: Draft | null };

export const EMPTY_LIVE: Live = { chatId: null, messages: [], draft: null };

/** Apply one streamed event to the live state. */
export function applyEvent(live: Live, event: AskEvent): Live {
  switch (event.type) {
    case "start":
      return { chatId: event.chat.id, messages: [...live.messages, event.question], draft: { text: "", chips: [] } };
    case "tool":
      return live.draft ? { ...live, draft: { ...live.draft, chips: [...live.draft.chips, event.chip] } } : live;
    case "text":
      return live.draft ? { ...live, draft: { ...live.draft, text: live.draft.text + event.text } } : live;
    case "reset":
      return live.draft ? { ...live, draft: { ...live.draft, text: "" } } : live;
    case "done":
    case "error":
      return { ...live, messages: [...live.messages, event.message], draft: null };
  }
}

/** Saved and live messages together, each once, in order. */
export function mergeMessages(saved: ChatMessage[], live: ChatMessage[]): ChatMessage[] {
  const byId = new Map<number, ChatMessage>();
  for (const m of [...saved, ...live]) byId.set(m.id, m);
  return [...byId.values()].sort((a, b) => a.id - b.id);
}

/** The question a failed answer was for, so "Try again" can ask it again. */
export function questionBefore(messages: ChatMessage[], index: number): string | null {
  for (let i = index - 1; i >= 0; i--) {
    if (messages[i].role === "user") return messages[i].content;
  }
  return null;
}

/** Links in answers: pages of this app stay in the app, https opens a new tab, anything else isn't a link. */
export function linkKind(href: string | undefined): "internal" | "external" | "none" {
  if (!href) return "none";
  if (href.startsWith("/") && !href.startsWith("//")) return "internal";
  if (href.startsWith("https://")) return "external";
  return "none";
}
