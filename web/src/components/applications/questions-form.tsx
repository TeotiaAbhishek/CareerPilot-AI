"use client";

import { Loader2, Send } from "lucide-react";
import { type FormEvent, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import type { AnswerSubmission, ApplicationQuestion } from "@/lib/api";

const CHOICE = new Set(["select", "radio", "checkbox"]);

/**
 * The questions the agent couldn't answer from your profile, answer bank or
 * resume. "Save to my answer bank" means no form asks you again.
 */
export function QuestionsForm({
  questions,
  onSubmit,
}: {
  questions: ApplicationQuestion[];
  onSubmit: (answers: AnswerSubmission[]) => Promise<void>;
}) {
  const [answers, setAnswers] = useState<Record<string, string>>({});
  // Long answers ("Why this role?") are usually job-specific, so they aren't saved by default.
  const [save, setSave] = useState<Record<string, boolean>>(() =>
    Object.fromEntries(
      questions.map((q) => [q.id, q.savable && q.kind !== "textarea"]),
    ),
  );
  const [busy, setBusy] = useState(false);

  const complete = questions.every((q) => (answers[q.id] ?? "").trim() !== "");

  async function submit(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    try {
      await onSubmit(
        questions.map((q) => ({
          question_id: q.id,
          answer: answers[q.id] ?? "",
          save: save[q.id] ?? true,
        })),
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-5">
      {questions.map((q) => {
        const id = `q-${q.id}`;
        const value = answers[q.id] ?? "";
        const set = (v: string) => setAnswers((a) => ({ ...a, [q.id]: v }));
        const options =
          q.kind === "checkbox" && q.options.length === 0
            ? ["Yes", "No"]
            : q.options;
        return (
          <div key={q.id} className="flex flex-col gap-1.5">
            <label htmlFor={id} className="text-sm font-medium">
              {q.label}
            </label>
            {CHOICE.has(q.kind) ? (
              <Select
                id={id}
                value={value}
                onChange={(e) => set(e.target.value)}
              >
                <option value="">Choose…</option>
                {options.map((o) => (
                  <option key={o} value={o}>
                    {o}
                  </option>
                ))}
              </Select>
            ) : q.kind === "textarea" ? (
              <Textarea
                id={id}
                value={value}
                rows={4}
                onChange={(e) => set(e.target.value)}
              />
            ) : (
              <Input
                id={id}
                value={value}
                type={
                  q.kind === "number" || q.kind === "date" || q.kind === "email"
                    ? q.kind
                    : "text"
                }
                onChange={(e) => set(e.target.value)}
              />
            )}
            {q.savable ? (
              <label className="flex items-center gap-2 text-xs text-muted-foreground">
                <input
                  type="checkbox"
                  className="size-3.5 accent-primary"
                  checked={save[q.id] ?? true}
                  onChange={(e) =>
                    setSave((s) => ({ ...s, [q.id]: e.target.checked }))
                  }
                />
                Save to my answer bank
              </label>
            ) : (
              <p className="text-xs text-muted-foreground">
                Only for this application.
              </p>
            )}
          </div>
        );
      })}
      <div>
        <Button type="submit" disabled={!complete || busy}>
          {busy ? <Loader2 className="animate-spin" /> : <Send />} Send answers
          and carry on
        </Button>
      </div>
    </form>
  );
}
