"use client";

import {
  ArrowLeft,
  CheckCircle2,
  ExternalLink,
  Loader2,
  MonitorUp,
  RotateCcw,
  TriangleAlert,
  XCircle,
  type LucideIcon,
} from "lucide-react";
import Link from "next/link";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import {
  type AnswerSubmission,
  type ApplicationView,
  type LiveEvent,
  answerQuestions,
  cancelApplication,
  errorMessage,
  focusApplication,
  markSubmitted,
  retryApplication,
  runEventsUrl,
} from "@/lib/api";
import { EVENT_TONE, MANUAL_REASON, STATUS, applyLiveEvent, isActive, lastEventId } from "@/lib/applications";
import { formatDateTime, timeAgo } from "@/lib/format";
import { cn } from "@/lib/utils";

import { QuestionsForm } from "./questions-form";

/** One apply run: what the agent is doing, live, and what it needs from you. */
// On WSL a window brought forward by a program can ignore clicks until you
// switch to it yourself, so the message says how.
const SHOWN = "Switched to the tab. If clicks don't respond in that window, Alt+Tab away and back once.";

export function RunView({ id }: { id: string }) {
  const { data, error, mutate, reload } = useApi<ApplicationView>(`/api/applications/${id}`);
  const [busy, setBusy] = useState(false);
  const logEnd = useRef<HTMLLIElement>(null);
  const loaded = data !== undefined;

  // The log streams in live (Server-Sent Events). A status change also
  // reloads the run, to pick up new questions and the message.
  const lastId = useRef(0);
  useEffect(() => {
    lastId.current = data ? lastEventId(data) : 0;
  }, [data]);
  useEffect(() => {
    if (!loaded) return;
    const source = new EventSource(runEventsUrl(id, lastId.current));
    source.onmessage = (message) => {
      const event = JSON.parse(message.data) as LiveEvent;
      mutate((view) => applyLiveEvent(view, event));
      if (event.type === "status") reload();
    };
    return () => source.close();
  }, [id, loaded, mutate, reload]);

  const count = data?.events.length ?? 0;
  useEffect(() => {
    logEnd.current?.scrollIntoView({ block: "nearest" });
  }, [count]);

  async function act(action: () => Promise<ApplicationView | void>, message: string) {
    setBusy(true);
    try {
      const fresh = await action();
      if (fresh) mutate(() => fresh);
      toast.success(message);
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function answer(answers: AnswerSubmission[]) {
    try {
      const fresh = await answerQuestions(id, answers);
      mutate(() => fresh);
      toast.success("Thanks. The agent carries on.");
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  if (error) {
    return <div className="rounded-xl border border-danger/30 bg-danger/5 p-6 text-sm text-danger">{error}</div>;
  }
  if (!data) return <Skeleton className="h-96 w-full" />;

  const status = STATUS[data.status];
  const open = data.questions.filter((q) => q.answer === null);
  const canShow = data.tab_id !== null && !["submitted", "cancelled"].includes(data.status);

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-3">
        <Link
          href={data.practice ? "/settings" : `/jobs/${encodeURIComponent(data.job_id)}`}
          className="inline-flex w-fit items-center gap-1 text-sm text-muted-foreground hover:text-foreground"
        >
          <ArrowLeft className="size-4" /> {data.practice ? "Settings" : "Back to the job"}
        </Link>
        <div className="flex flex-wrap items-center gap-3">
          <h1 className="text-xl font-semibold tracking-tight">
            {data.job_title}
            {data.company ? <span className="text-muted-foreground"> · {data.company}</span> : null}
          </h1>
          <Badge tone={status.tone}>
            {isActive(data.status) ? <Loader2 className="size-3 animate-spin" aria-hidden /> : null}
            {status.label}
          </Badge>
          {data.manual_reason ? <Badge tone="warning">{MANUAL_REASON[data.manual_reason]}</Badge> : null}
          {data.practice ? <Badge>Practice</Badge> : null}
        </div>
        <p className="text-sm text-muted-foreground">
          Started {timeAgo(data.created_at)} · {data.steps} step{data.steps === 1 ? "" : "s"}
          {data.current_url ? (
            <>
              {" · "}
              <a href={data.current_url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 hover:underline">
                {new URL(data.current_url).host} <ExternalLink className="size-3" />
              </a>
            </>
          ) : null}
        </p>
      </div>

      {data.status === "ready_to_submit" ? (
        <Callout tone="success" icon={CheckCircle2} title="Ready for you to submit">
          The form is filled in. Check every answer in the agent&apos;s browser window, change anything you like, then
          click Submit there yourself. Come back and mark it submitted.
        </Callout>
      ) : data.status === "needs_manual" ? (
        <Callout tone="warning" icon={TriangleAlert} title="This one needs you">
          {data.message}
        </Callout>
      ) : data.status === "failed" ? (
        <Callout tone="danger" icon={XCircle} title="The run failed">
          {data.message}
        </Callout>
      ) : null}

      <div className="flex flex-wrap gap-2">
        {canShow ? (
          <Button variant="outline" disabled={busy} onClick={() => act(() => focusApplication(id), SHOWN)}>
            <MonitorUp /> Show in the browser
          </Button>
        ) : null}
        {["ready_to_submit", "needs_manual", "waiting_for_you"].includes(data.status) ? (
          <Button disabled={busy} onClick={() => act(() => markSubmitted(id), "Marked as submitted")}>
            <CheckCircle2 /> {data.status === "ready_to_submit" ? "I submitted it" : "I finished it myself"}
          </Button>
        ) : null}
        {["needs_manual", "failed"].includes(data.status) ? (
          <Button variant="outline" disabled={busy} onClick={() => act(() => retryApplication(id), "Trying again")}>
            <RotateCcw /> Try again
          </Button>
        ) : null}
        {!["submitted", "cancelled"].includes(data.status) ? (
          <Button variant="ghost" disabled={busy} onClick={() => act(() => cancelApplication(id), "Cancelled")}>
            <XCircle /> Cancel
          </Button>
        ) : null}
        {data.status === "submitted" ? (
          <Link href="/applications" className={buttonVariants({ variant: "outline" })}>
            All applications
          </Link>
        ) : null}
      </div>

      {data.status === "waiting_for_you" && open.length > 0 ? (
        <Card className="border-warning/40">
          <CardHeader>
            <CardTitle>
              {open.length} question{open.length === 1 ? "" : "s"} for you
            </CardTitle>
            <p className="text-sm text-muted-foreground">
              Nothing in your profile, answer bank or resume answers {open.length === 1 ? "this" : "these"}. The form
              stays open in its tab while you answer.
            </p>
          </CardHeader>
          <CardContent>
            <QuestionsForm key={open.map((q) => q.id).join()} questions={open} onSubmit={answer} />
          </CardContent>
        </Card>
      ) : null}

      <Card>
        <CardHeader>
          <CardTitle>What the agent did</CardTitle>
        </CardHeader>
        <CardContent>
          {data.events.length === 0 ? (
            <p className="text-sm text-muted-foreground">Nothing yet.</p>
          ) : (
            <ol className="flex max-h-[28rem] flex-col gap-1.5 overflow-y-auto font-mono text-xs leading-5" aria-live="polite">
              {data.events.map((event) => (
                <li key={event.id} className="flex gap-3">
                  <time className="shrink-0 text-muted-foreground tabular-nums" dateTime={event.at} title={formatDateTime(event.at)}>
                    {new Date(event.at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                  </time>
                  <span className={cn("min-w-0 break-words", EVENT_TONE[event.kind])}>{event.message}</span>
                </li>
              ))}
              <li ref={logEnd} aria-hidden />
            </ol>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function Callout({
  tone,
  icon: Icon,
  title,
  children,
}: {
  tone: "success" | "warning" | "danger";
  icon: LucideIcon;
  title: string;
  children: ReactNode;
}) {
  const styles = {
    success: "border-success/30 bg-success/10",
    warning: "border-warning/30 bg-warning/10",
    danger: "border-danger/30 bg-danger/10",
  }[tone];
  const iconTone = { success: "text-success", warning: "text-warning", danger: "text-danger" }[tone];
  return (
    <div className={cn("flex gap-3 rounded-lg border p-4 text-sm", styles)}>
      <Icon className={cn("mt-0.5 size-4 shrink-0", iconTone)} aria-hidden />
      <div>
        <p className="font-medium">{title}</p>
        <p className="mt-1 text-muted-foreground">{children}</p>
      </div>
    </div>
  );
}
