"use client";

import { Ban, Check, CircleHelp, Loader2, RefreshCw, TriangleAlert, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import type { JobDetail, JobLabel } from "@/lib/api";
import { type ScoreTone, scoreTone, seconds } from "@/lib/scoring";
import { cn } from "@/lib/utils";

import { LabelControl } from "./label-control";

const TONE_CLASSES: Record<ScoreTone, string> = {
  success: "border-success/30 bg-success/10 text-success",
  warning: "border-warning/30 bg-warning/10 text-warning",
  danger: "border-danger/30 bg-danger/10 text-danger",
  neutral: "bg-muted text-muted-foreground",
};

/**
 * Why the model gave this score, plus your Apply / Maybe / No label.
 * Shown at the top of a job's detail.
 */
export function ScorePanel({
  job,
  onLabel,
  onRescore,
}: {
  job: JobDetail;
  onLabel: (label: JobLabel | null) => void;
  onRescore: () => void;
}) {
  const score = job.score;
  const tone = scoreTone(score?.score ?? null, score?.hard_blockers.length ?? null, job.relevant);

  return (
    <section aria-label="Relevance" className="border-b p-6">
      <div className="flex items-start gap-4">
        <div
          className={cn(
            "flex size-14 shrink-0 flex-col items-center justify-center rounded-xl border",
            TONE_CLASSES[tone],
          )}
        >
          <span className="text-xl font-semibold leading-none tabular-nums">{score?.score ?? "–"}</span>
          <span className="mt-1 text-[10px] font-medium uppercase tracking-wide opacity-80">score</span>
        </div>

        <div className="min-w-0 flex-1">
          <h3 className="text-sm font-semibold">
            {score === null
              ? "Not scored yet"
              : job.relevant
                ? "Relevant to you"
                : score.hard_blockers.length > 0
                  ? "Blocked"
                  : "Below your threshold"}
          </h3>
          {score ? (
            <p className="mt-0.5 text-xs text-muted-foreground">
              {score.model} · prompt {score.prompt_version} · {seconds(score.duration_ms)}
              {job.legacy_score !== null ? ` · v1 gave ${job.legacy_score}` : ""}
            </p>
          ) : job.legacy_score !== null ? (
            <p className="mt-0.5 text-xs text-muted-foreground">v1 gave {job.legacy_score}</p>
          ) : null}
        </div>

        <Button
          variant="outline"
          size="sm"
          onClick={onRescore}
          disabled={job.score_pending}
          aria-label={job.score_pending ? "Scoring" : "Re-score"}
        >
          {job.score_pending ? <Loader2 className="animate-spin" /> : <RefreshCw />}
          <span className="hidden sm:inline">{job.score_pending ? "Scoring…" : "Re-score"}</span>
        </Button>
      </div>

      {score ? (
        <>
          <p className="mt-4 text-sm leading-relaxed">{score.summary}</p>
          <div className="mt-4 grid gap-4 sm:grid-cols-2">
            <ItemList title="Hard blockers" items={score.hard_blockers} icon={Ban} className="text-danger" />
            <ItemList title="Matched" items={score.matched} icon={Check} className="text-success" />
            <ItemList title="Missing" items={score.missing} icon={X} className="text-warning" />
            <ItemList title="Not in your resume" items={score.unknown} icon={CircleHelp} className="text-muted-foreground" />
          </div>
        </>
      ) : job.score_error && !job.score_pending ? (
        <p className="mt-4 flex gap-2 rounded-lg border border-warning/30 bg-warning/10 p-3 text-sm">
          <TriangleAlert className="size-4 shrink-0 text-warning" aria-hidden />
          <span>Scoring failed: {job.score_error}. Try Re-score.</span>
        </p>
      ) : (
        <p className="mt-4 flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" aria-hidden />
          Waiting for the local model. This page updates by itself.
        </p>
      )}

      <div className="mt-5 flex flex-wrap items-center gap-3">
        <span className="text-sm text-muted-foreground">Would you apply?</span>
        <LabelControl value={job.label} onChange={onLabel} />
      </div>
    </section>
  );
}

function ItemList({
  title,
  items,
  icon: Icon,
  className,
}: {
  title: string;
  items: string[];
  icon: typeof Check;
  className: string;
}) {
  if (items.length === 0) return null;
  return (
    <div>
      <h4 className="text-xs font-medium uppercase tracking-wide text-muted-foreground">{title}</h4>
      <ul className="mt-2 flex flex-col gap-1.5">
        {items.map((item) => (
          <li key={item} className="flex gap-2 text-sm">
            <Icon className={cn("mt-0.5 size-4 shrink-0", className)} aria-hidden />
            <span>{item}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
