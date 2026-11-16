"use client";

import { Bookmark, Zap } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import type { JobSummary } from "@/lib/api";
import { timeAgo } from "@/lib/format";
import { LABEL_TONE, LABELS } from "@/lib/scoring";
import { cn } from "@/lib/utils";

import { ScoreBadge } from "./score-badge";

export function JobList({
  jobs,
  selectedId,
  onSelect,
  checked,
  onCheck,
}: {
  jobs: JobSummary[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  /** Jobs ticked for "Apply to selected". */
  checked: Set<string>;
  onCheck: (id: string, on: boolean) => void;
}) {
  return (
    <ul className="flex flex-col gap-2" aria-label="Jobs">
      {jobs.map((job) => {
        const selected = job.id === selectedId;
        const unread = job.viewed_at === null;
        return (
          <li key={job.id} className="flex items-stretch gap-2">
            <label className="flex shrink-0 cursor-pointer items-start pt-4.5 pl-1">
              <input
                type="checkbox"
                className="size-4 accent-primary"
                checked={checked.has(job.id)}
                onChange={(e) => onCheck(job.id, e.target.checked)}
                aria-label={`Select ${job.title}`}
              />
            </label>
            <button
              type="button"
              id={`job-${job.id}`}
              onClick={() => onSelect(job.id)}
              aria-current={selected ? "true" : undefined}
              className={cn(
                "group min-w-0 flex-1 rounded-lg border bg-card p-4 text-left transition-colors hover:border-primary/40",
                selected && "border-primary ring-1 ring-primary",
              )}
            >
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <p className={cn("truncate text-sm", unread ? "font-semibold" : "font-medium")}>
                    {unread ? (
                      <span className="mr-2 inline-block size-2 rounded-full bg-primary align-middle" aria-label="New" />
                    ) : null}
                    {job.title}
                  </p>
                  <p className="mt-0.5 truncate text-sm text-muted-foreground">
                    {[job.company, job.location].filter(Boolean).join(" · ")}
                  </p>
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  {job.saved ? (
                    <Bookmark className="size-4 fill-primary text-primary" aria-label="Saved" />
                  ) : null}
                  <ScoreBadge score={job.score} blockers={job.blocker_count} relevant={job.relevant} />
                </div>
              </div>

              <div className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                {job.work_type ? <span>{job.work_type}</span> : null}
                {job.salary ? <span className="truncate">{job.salary}</span> : null}
                {job.apply_type?.toLowerCase().includes("quick") ? (
                  <span className="inline-flex items-center gap-1 text-primary">
                    <Zap className="size-3" aria-hidden /> Quick apply
                  </span>
                ) : null}
                {job.label ? (
                  <Badge tone={LABEL_TONE[job.label]} className="py-0">
                    {LABELS.find((l) => l.value === job.label)?.text}
                  </Badge>
                ) : null}
                <span className="ml-auto">{job.posted_label ?? timeAgo(job.first_seen_at)}</span>
              </div>
            </button>
          </li>
        );
      })}
    </ul>
  );
}

export function JobListSkeleton() {
  return (
    <div className="flex flex-col gap-2" aria-busy="true" aria-label="Loading jobs">
      {Array.from({ length: 6 }, (_, i) => (
        <div key={i} className="rounded-lg border bg-card p-4">
          <Skeleton className="h-4 w-2/3" />
          <Skeleton className="mt-2 h-3 w-1/2" />
          <Skeleton className="mt-4 h-3 w-1/3" />
        </div>
      ))}
    </div>
  );
}

export function Pagination({
  page,
  pageSize,
  total,
  onPage,
}: {
  page: number;
  pageSize: number;
  total: number;
  onPage: (page: number) => void;
}) {
  const from = total === 0 ? 0 : (page - 1) * pageSize + 1;
  const to = Math.min(page * pageSize, total);
  const last = Math.max(1, Math.ceil(total / pageSize));

  return (
    <div className="mt-4 flex items-center justify-between text-sm text-muted-foreground">
      <span className="tabular-nums">
        {from}–{to} of {total}
      </span>
      <div className="flex gap-2">
        <Button variant="outline" size="sm" disabled={page <= 1} onClick={() => onPage(page - 1)}>
          Previous
        </Button>
        <Button variant="outline" size="sm" disabled={page >= last} onClick={() => onPage(page + 1)}>
          Next
        </Button>
      </div>
    </div>
  );
}
