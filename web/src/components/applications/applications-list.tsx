"use client";

import { ArrowRight, KanbanSquare, Loader2 } from "lucide-react";
import Link from "next/link";
import { useEffect } from "react";

import { Badge } from "@/components/ui/badge";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import type { Application } from "@/lib/api";
import { MANUAL_REASON, STATUS, isActive, sortForList } from "@/lib/applications";
import { timeAgo } from "@/lib/format";

const POLL_MS = 5000;

/** Every apply run, your turn first. (The full pipeline board comes in F7.) */
export function ApplicationsList() {
  const { data, error, loading, reload } = useApi<Application[]>("/api/applications");

  const active = data?.some((a) => isActive(a.status)) ?? false;
  useEffect(() => {
    if (!active) return;
    const timer = setInterval(reload, POLL_MS);
    return () => clearInterval(timer);
  }, [active, reload]);

  if (error) {
    return <div className="rounded-xl border border-danger/30 bg-danger/5 p-6 text-sm text-danger">{error}</div>;
  }
  if (loading && !data) {
    return (
      <div className="flex flex-col gap-3">
        {[0, 1, 2].map((i) => (
          <Skeleton key={i} className="h-16 w-full" />
        ))}
      </div>
    );
  }
  if (!data?.length) {
    return (
      <EmptyState
        icon={KanbanSquare}
        title="No applications yet"
        description="Approve a tailored resume on a job, then click Apply with the agent. Every run shows up here."
        action={
          <Link href="/jobs?sort=best_fit" className="text-sm font-medium text-primary hover:underline">
            Find a job to apply to
          </Link>
        }
      />
    );
  }

  return (
    <ul className="flex flex-col divide-y rounded-xl border bg-card">
      {sortForList(data).map((a) => (
        <li key={a.id}>
          <Link
            href={`/applications/${a.id}`}
            className="flex flex-wrap items-center gap-x-4 gap-y-2 p-4 transition-colors hover:bg-muted/50"
          >
            <div className="min-w-0 flex-1">
              <p className="truncate font-medium">
                {a.job_title}
                {a.company ? <span className="text-muted-foreground"> · {a.company}</span> : null}
              </p>
              <p className="truncate text-sm text-muted-foreground">
                {a.open_questions > 0
                  ? `${a.open_questions} question${a.open_questions === 1 ? "" : "s"} for you`
                  : (a.message ?? `Started ${timeAgo(a.created_at)}`)}
              </p>
            </div>
            <div className="flex items-center gap-2">
              {a.practice ? <Badge>Practice</Badge> : null}
              {a.manual_reason ? <Badge tone="warning">{MANUAL_REASON[a.manual_reason]}</Badge> : null}
              <Badge tone={STATUS[a.status].tone}>
                {isActive(a.status) ? <Loader2 className="size-3 animate-spin" aria-hidden /> : null}
                {STATUS[a.status].label}
              </Badge>
              <span className="w-16 text-right text-xs text-muted-foreground tabular-nums">{timeAgo(a.updated_at)}</span>
              <ArrowRight className="size-4 text-muted-foreground" aria-hidden />
            </div>
          </Link>
        </li>
      ))}
    </ul>
  );
}
