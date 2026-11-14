"use client";

import { Bot, MessagesSquare, Pencil, Plus, Search, Trash2 } from "lucide-react";
import { type FormEvent, useEffect, useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Textarea } from "@/components/ui/textarea";
import { useApi } from "@/hooks/use-api";
import {
  type Answer,
  type AnswerInput,
  createAnswer,
  deleteAnswer,
  errorMessage,
  updateAnswer,
} from "@/lib/api";
import { timeAgo } from "@/lib/format";

/**
 * Your answers to screening questions. The F6 form-fill agent searches these,
 * and saves new answers you give it here (marked "agent").
 */
export function AnswersManager() {
  const [text, setText] = useState("");
  const [q, setQ] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [adding, setAdding] = useState(false);
  const { data, error, reload } = useApi<Answer[]>(
    q ? `/api/answers?q=${encodeURIComponent(q)}` : "/api/answers",
  );

  // Search 300 ms after the last key press.
  useEffect(() => {
    const id = setTimeout(() => setQ(text.trim()), 300);
    return () => clearTimeout(id);
  }, [text]);

  async function remove(answer: Answer) {
    try {
      await deleteAnswer(answer.id);
      toast.success("Answer deleted");
      reload();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap gap-2">
        <div className="relative min-w-60 flex-1">
          <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" aria-hidden />
          <Input
            type="search"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder="Search questions and answers…"
            aria-label="Search answers"
            className="pl-9"
          />
        </div>
        <Button onClick={() => setAdding(true)} disabled={adding}>
          <Plus /> Add answer
        </Button>
      </div>

      {adding ? (
        <AnswerForm
          title="New answer"
          onCancel={() => setAdding(false)}
          onSave={async (input) => {
            await createAnswer(input);
            toast.success("Answer saved");
            setAdding(false);
            reload();
          }}
        />
      ) : null}

      {error ? (
        <p className="text-sm text-danger">{error}</p>
      ) : !data ? (
        <Skeleton className="h-40 w-full" />
      ) : data.length === 0 ? (
        <EmptyState
          icon={MessagesSquare}
          title={q ? "No answers match" : "No answers yet"}
          description={
            q
              ? "Try other words."
              : "Add answers to questions forms keep asking: driver's licence, police check, why this company… The apply agent will use them."
          }
        />
      ) : (
        <ul className="flex flex-col gap-3">
          {data.map((a) =>
            editing === a.id ? (
              <li key={a.id}>
                <AnswerForm
                  title="Edit answer"
                  initial={a}
                  onCancel={() => setEditing(null)}
                  onSave={async (input) => {
                    await updateAnswer(a.id, input);
                    toast.success("Answer updated");
                    setEditing(null);
                    reload();
                  }}
                />
              </li>
            ) : (
              <li key={a.id} className="rounded-xl border bg-card p-4">
                <div className="flex items-start justify-between gap-3">
                  <p className="font-medium">{a.question}</p>
                  <div className="flex shrink-0 gap-1">
                    <Button variant="ghost" size="icon" aria-label="Edit answer" onClick={() => setEditing(a.id)}>
                      <Pencil />
                    </Button>
                    <ConfirmDelete onConfirm={() => remove(a)} />
                  </div>
                </div>
                <p className="mt-1 whitespace-pre-line text-sm text-muted-foreground">{a.answer}</p>
                <div className="mt-3 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                  {a.source === "agent" ? (
                    <Badge tone="primary">
                      <Bot className="size-3" aria-hidden /> From the agent
                    </Badge>
                  ) : null}
                  {a.tags.map((t) => (
                    <Badge key={t}>{t}</Badge>
                  ))}
                  <span className="ml-auto">
                    {a.times_used > 0 ? `Used ${a.times_used}× · ` : ""}updated {timeAgo(a.updated_at)}
                  </span>
                </div>
              </li>
            ),
          )}
        </ul>
      )}
    </div>
  );
}

function AnswerForm({
  title,
  initial,
  onSave,
  onCancel,
}: {
  title: string;
  initial?: Answer;
  onSave: (input: AnswerInput) => Promise<void>;
  onCancel: () => void;
}) {
  const [question, setQuestion] = useState(initial?.question ?? "");
  const [answer, setAnswer] = useState(initial?.answer ?? "");
  const [tags, setTags] = useState(initial?.tags.join(", ") ?? "");
  const [saving, setSaving] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    try {
      await onSave({ question, answer, tags: tags.split(",").map((t) => t.trim()).filter(Boolean) });
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        <CardDescription>Write it the way you&apos;d want it pasted into a form.</CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={submit} className="flex flex-col gap-3">
          <Input
            aria-label="Question"
            placeholder="Question, e.g. Do you have a current driver's licence?"
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
            required
            autoFocus
          />
          <Textarea
            aria-label="Answer"
            placeholder="Your answer"
            value={answer}
            onChange={(e) => setAnswer(e.target.value)}
            rows={4}
            required
          />
          <Input
            aria-label="Tags"
            placeholder="Tags, comma separated (optional): licence, checks"
            value={tags}
            onChange={(e) => setTags(e.target.value)}
          />
          <div className="flex justify-end gap-2">
            <Button variant="ghost" onClick={onCancel}>
              Cancel
            </Button>
            <Button type="submit" disabled={saving || !question.trim() || !answer.trim()}>
              Save
            </Button>
          </div>
        </form>
      </CardContent>
    </Card>
  );
}

/** A bin button that asks "Delete?" once before doing it. */
export function ConfirmDelete({ onConfirm, label = "Delete" }: { onConfirm: () => void; label?: string }) {
  const [armed, setArmed] = useState(false);

  useEffect(() => {
    if (!armed) return;
    const id = setTimeout(() => setArmed(false), 3000);
    return () => clearTimeout(id);
  }, [armed]);

  return armed ? (
    <Button variant="danger" size="sm" onClick={onConfirm}>
      {label}?
    </Button>
  ) : (
    <Button variant="ghost" size="icon" aria-label={label} onClick={() => setArmed(true)}>
      <Trash2 />
    </Button>
  );
}
