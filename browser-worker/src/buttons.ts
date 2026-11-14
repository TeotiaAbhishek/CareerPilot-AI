import type { ButtonKind } from "./types.ts";

// Anything that could send the application. The agent is never allowed to
// click these: you click the final Submit yourself. When in doubt, a button
// counts as "submit", because stopping one step early is harmless and
// submitting by mistake is not.
const SUBMIT = /\b(submit|send|apply|finish|complete|confirm)\b/i;
const NEXT = /^(next|continue|save (and|&) (continue|next)|proceed|go to next|start|begin|review)\b/i;
const BACK = /^(back|previous|cancel|close|edit)\b/i;

export function classifyButton(text: string): ButtonKind {
  const t = text.trim();
  if (SUBMIT.test(t)) return "submit";
  if (NEXT.test(t)) return "next";
  if (BACK.test(t)) return "back";
  return "other";
}
