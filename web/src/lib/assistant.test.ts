import { describe, expect, it } from "vitest";

import type { ChatMessage } from "@/lib/api";
import { splitLines } from "@/lib/api";
import { EMPTY_LIVE, applyEvent, linkKind, mergeMessages, questionBefore } from "@/lib/assistant";

const message = (id: number, role: "user" | "assistant", content: string, failed = false): ChatMessage => ({
  id,
  role,
  content,
  tools: [],
  failed,
  created_at: "2026-11-16T00:00:00Z",
});
const chat = { id: "c1", title: "Hi", created_at: "", updated_at: "" };

describe("applyEvent", () => {
  it("builds the answer from streamed events", () => {
    let live = applyEvent(EMPTY_LIVE, { type: "start", chat, question: message(1, "user", "Which first?") });
    live = applyEvent(live, { type: "text", text: "Let me check." });
    live = applyEvent(live, { type: "reset" });
    live = applyEvent(live, { type: "tool", chip: { tool: "search_jobs", label: "Looked at 3 jobs" } });
    live = applyEvent(live, { type: "text", text: "Start " });
    live = applyEvent(live, { type: "text", text: "with Globex." });
    expect(live.chatId).toBe("c1");
    expect(live.draft).toEqual({ text: "Start with Globex.", chips: [{ tool: "search_jobs", label: "Looked at 3 jobs" }] });

    live = applyEvent(live, { type: "done", message: message(2, "assistant", "Start with Globex.") });
    expect(live.draft).toBeNull();
    expect(live.messages.map((m) => m.id)).toEqual([1, 2]);
  });
});

describe("mergeMessages", () => {
  it("keeps each message once, in order", () => {
    const merged = mergeMessages([message(1, "user", "a"), message(2, "assistant", "b")], [message(2, "assistant", "b"), message(3, "user", "c")]);
    expect(merged.map((m) => m.id)).toEqual([1, 2, 3]);
  });
});

describe("questionBefore", () => {
  it("finds the question a failed answer belongs to", () => {
    const messages = [message(1, "user", "first"), message(2, "assistant", "ok"), message(3, "user", "second"), message(4, "assistant", "down", true)];
    expect(questionBefore(messages, 3)).toBe("second");
    expect(questionBefore(messages, 0)).toBeNull();
  });
});

describe("linkKind", () => {
  it("only trusts app pages and https", () => {
    expect(linkKind("/jobs/123")).toBe("internal");
    expect(linkKind("//evil.example")).toBe("none");
    expect(linkKind("https://www.seek.com.au/job/1")).toBe("external");
    expect(linkKind("javascript:alert(1)")).toBe("none");
    expect(linkKind(undefined)).toBe("none");
  });
});

describe("splitLines", () => {
  it("keeps a half-received line for later", () => {
    const { events, rest } = splitLines('{"type":"reset"}\n{"type":"te');
    expect(events).toEqual([{ type: "reset" }]);
    expect(rest).toBe('{"type":"te');
  });
});
