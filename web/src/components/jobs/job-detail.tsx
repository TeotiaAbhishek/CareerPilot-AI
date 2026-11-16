"use client";

import {
  Bookmark,
  BriefcaseBusiness,
  Clock,
  DollarSign,
  ExternalLink,
  EyeOff,
  MapPin,
  TriangleAlert,
  Zap,
} from "lucide-react";
import { useEffect, useEffectEvent, useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import {
  type JobDetail,
  type JobLabel,
  type JobPatch,
  apiGet,
  errorMessage,
  patchJob,
  rescoreJob,
  setJobLabel,
} from "@/lib/api";
import { formatDateTime, timeAgo } from "@/lib/format";
import { cn } from "@/lib/utils";

import { ApplyPanel } from "@/components/applications/apply-panel";
import { PreparePanel } from "@/components/documents/prepare-panel";
import { EmailPanel } from "@/components/emails/email-panel";

import { ScoreBadge } from "./score-badge";
import { ScorePanel } from "./score-panel";

/** How often to check for a score while one is on its way. */
const SCORE_POLL_MS = 5000;

/**
 * One job in full. Used both in the right-hand pane of /jobs and on /jobs/[id].
 * Opening a job marks it as viewed. `onChange` tells the list about any change.
 * With `shortcuts`, the a / m / n keys label the job Apply / Maybe / No.
 */
export function JobDetailView({
  id,
  onChange,
  shortcuts = false,
}: {
  id: string;
  onChange?: (job: JobDetail) => void;
  shortcuts?: boolean;
}) {
  const path = `/api/jobs/${encodeURIComponent(id)}`;
  const { data: job, error, loading, mutate } = useApi<JobDetail>(path);
  const current = job?.id === id ? job : undefined;
  // Bumped when one-click Apply queues a draft, so the documents panel loads it.
  const [docsVersion, setDocsVersion] = useState(0);

  // While a score is on its way, check every few seconds. (A quiet fetch,
  // not reload(), so the page doesn't flash a skeleton each time.)
  const waiting = !!current && (current.score_pending || (!current.score && !current.score_error));
  useEffect(() => {
    if (!waiting) return;
    const timer = setInterval(() => {
      apiGet<JobDetail>(path, { cache: "no-store" }).then(
        (updated) => {
          mutate(() => updated);
          if (updated.score && !updated.score_pending) onChange?.(updated);
        },
        () => {},
      );
    }, SCORE_POLL_MS);
    return () => clearInterval(timer);
  }, [waiting, path, mutate, onChange]);

  // Mark as viewed the first time it's opened.
  useEffect(() => {
    if (!current || current.viewed_at) return;
    patchJob(current.id, { viewed: true }).then(
      (updated) => {
        mutate(() => updated);
        onChange?.(updated);
      },
      () => {
        // Not worth bothering the user about.
      },
    );
  }, [current, mutate, onChange]);

  async function update(patch: JobPatch, success: string, undo?: JobPatch) {
    if (!current) return;
    try {
      const updated = await patchJob(current.id, patch);
      mutate(() => updated);
      onChange?.(updated);
      toast.success(success, {
        action: undo
          ? { label: "Undo", onClick: () => void update(undo, "Undone") }
          : undefined,
      });
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  async function label(value: JobLabel | null) {
    if (!current) return;
    const previous = current.label;
    const updated = { ...current, label: value };
    // Show it straight away; put it back if the API says no.
    mutate(() => updated);
    onChange?.(updated);
    try {
      await setJobLabel(current.id, value);
    } catch (e) {
      mutate((j) => ({ ...j, label: previous }));
      onChange?.({ ...updated, label: previous });
      toast.error(errorMessage(e));
    }
  }

  async function rescore() {
    if (!current) return;
    try {
      await rescoreJob(current.id);
      mutate((j) => ({ ...j, score_pending: true }));
      toast.success("Queued for scoring");
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  // Keyboard labelling. useEffectEvent always sees the latest job and `label`,
  // without re-adding the listener on every render.
  const onLabelKey = useEffectEvent((value: JobLabel) => {
    if (current) void label(current.label === value ? null : value);
  });
  useEffect(() => {
    if (!shortcuts) return;
    function onKey(event: KeyboardEvent) {
      const target = event.target as HTMLElement;
      if (target.closest("input, select, textarea")) return;
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      const keys: Record<string, JobLabel> = { a: "apply", m: "maybe", n: "no" };
      const value = keys[event.key];
      if (!value) return;
      event.preventDefault();
      onLabelKey(value);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [shortcuts]);

  if (error) {
    return (
      <div className="rounded-xl border border-danger/30 bg-danger/5 p-6 text-sm text-danger">
        {error}
      </div>
    );
  }
  if (loading || !current) return <JobDetailSkeleton />;

  const quickApply = current.apply_type?.toLowerCase().includes("quick");

  return (
    <article className="rounded-xl border bg-card">
      <header className="border-b p-6">
        <div className="flex flex-wrap items-center gap-2">
          {quickApply ? (
            <Badge tone="primary">
              <Zap className="size-3" aria-hidden /> Quick apply
            </Badge>
          ) : null}
          <ScoreBadge
            score={current.score?.score ?? null}
            blockers={current.score?.hard_blockers.length ?? null}
            relevant={current.relevant}
          />
          {current.hidden ? <Badge tone="warning">Hidden</Badge> : null}
        </div>

        <h2 className="mt-3 text-xl font-semibold tracking-tight">{current.title}</h2>
        <p className="mt-1 text-muted-foreground">{current.company ?? "Company not listed"}</p>

        <dl className="mt-4 grid gap-2 text-sm text-muted-foreground sm:grid-cols-2">
          <Meta icon={MapPin} label="Location" value={current.location} />
          <Meta icon={BriefcaseBusiness} label="Work type" value={current.work_type} />
          <Meta icon={DollarSign} label="Salary" value={current.salary} />
          <Meta
            icon={Clock}
            label="Posted"
            value={current.posted_label ? `Posted ${current.posted_label}` : `Found ${timeAgo(current.first_seen_at)}`}
          />
        </dl>

        <div className="mt-5 flex flex-wrap gap-2">
          <a
            href={current.job_url}
            target="_blank"
            rel="noopener noreferrer"
            className={buttonVariants()}
          >
            <ExternalLink /> View on SEEK
          </a>
          <Button
            variant="outline"
            aria-pressed={current.saved}
            onClick={() =>
              update(
                { saved: !current.saved },
                current.saved ? "Removed from saved" : "Saved",
              )
            }
          >
            <Bookmark className={cn(current.saved && "fill-primary text-primary")} />
            {current.saved ? "Saved" : "Save"}
          </Button>
          <Button
            variant="ghost"
            onClick={() =>
              current.hidden
                ? update({ hidden: false }, "Job is visible again")
                : update({ hidden: true }, "Job hidden", { hidden: false })
            }
          >
            <EyeOff /> {current.hidden ? "Unhide" : "Hide"}
          </Button>
        </div>
      </header>

      <ScorePanel job={current} onLabel={label} onRescore={rescore} />

      <PreparePanel key={docsVersion} jobId={current.id} canDraft={!!current.description_text} />

      <ApplyPanel
        jobId={current.id}
        applyType={current.apply_type}
        onStarted={() => setDocsVersion((v) => v + 1)}
      />

      <EmailPanel jobId={current.id} />

      {current.scrape_status === "failed" ? (
        <div className="m-6 mb-0 flex gap-3 rounded-lg border border-warning/30 bg-warning/10 p-4 text-sm">
          <TriangleAlert className="size-4 shrink-0 text-warning" aria-hidden />
          <p>
            We couldn&apos;t load this job&apos;s full description
            {current.scrape_error ? ` (${current.scrape_error})` : ""}. The next scrape will try
            again, or open it on SEEK.
          </p>
        </div>
      ) : null}

      {current.description_html ? (
        <div
          className="prose prose-sm max-w-none p-6 dark:prose-invert prose-headings:font-semibold prose-a:text-primary"
          // Safe: the backend cleans this HTML with ammonia before storing it.
          dangerouslySetInnerHTML={{ __html: current.description_html }}
        />
      ) : current.description_text ? (
        <p className="whitespace-pre-line p-6 text-sm leading-relaxed">{current.description_text}</p>
      ) : (
        <p className="p-6 text-sm text-muted-foreground">No description.</p>
      )}

      <footer className="border-t px-6 py-3 text-xs text-muted-foreground">
        SEEK job {current.id} · first found {formatDateTime(current.first_seen_at)} · last seen{" "}
        {timeAgo(current.last_seen_at)}
      </footer>
    </article>
  );
}

function Meta({
  icon: Icon,
  label,
  value,
}: {
  icon: typeof MapPin;
  label: string;
  value: string | null;
}) {
  if (!value) return null;
  return (
    <div className="flex items-center gap-2">
      <dt className="sr-only">{label}</dt>
      <Icon className="size-4 shrink-0" aria-hidden />
      <dd className="truncate">{value}</dd>
    </div>
  );
}

export function JobDetailSkeleton() {
  return (
    <div className="rounded-xl border bg-card p-6" aria-busy="true">
      <Skeleton className="h-5 w-24" />
      <Skeleton className="mt-4 h-6 w-3/4" />
      <Skeleton className="mt-2 h-4 w-1/3" />
      <div className="mt-6 grid gap-2 sm:grid-cols-2">
        <Skeleton className="h-4" />
        <Skeleton className="h-4" />
        <Skeleton className="h-4" />
        <Skeleton className="h-4" />
      </div>
      <Skeleton className="mt-8 h-40 w-full" />
    </div>
  );
}
