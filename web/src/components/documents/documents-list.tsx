"use client";

import { ArrowRight, FileText } from "lucide-react";
import Link from "next/link";
import { useEffect } from "react";

import { Badge } from "@/components/ui/badge";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import type { DocumentKind, DocumentSummary } from "@/lib/api";
import { KIND_LABEL, STATUS, isPending } from "@/lib/documents";
import { timeAgo } from "@/lib/format";

const POLL_MS = 5000;

type JobGroup = {
  job_id: string;
  job_title: string;
  company: string | null;
  updated_at: string;
  docs: Partial<Record<DocumentKind, DocumentSummary>>;
};

/** One row per job, newest activity first. */
export function groupByJob(rows: DocumentSummary[]): JobGroup[] {
  const groups = new Map<string, JobGroup>();
  for (const row of rows) {
    const group = groups.get(row.job_id) ?? {
      job_id: row.job_id,
      job_title: row.job_title,
      company: row.company,
      updated_at: row.updated_at,
      docs: {},
    };
    group.docs[row.kind] = row;
    if (row.updated_at > group.updated_at) group.updated_at = row.updated_at;
    groups.set(row.job_id, group);
  }
  return [...groups.values()].sort((a, b) => b.updated_at.localeCompare(a.updated_at));
}

export function DocumentsList() {
  const { data, error, loading, reload } = useApi<DocumentSummary[]>("/api/documents");

  const pending = data?.some((d) => isPending(d.status)) ?? false;
  useEffect(() => {
    if (!pending) return;
    const timer = setInterval(reload, POLL_MS);
    return () => clearInterval(timer);
  }, [pending, reload]);

  if (error) {
    return <div className="rounded-xl border border-danger/30 bg-danger/5 p-6 text-sm text-danger">{error}</div>;
  }
  if (loading && !data) {
    return (
      <div className="flex flex-col gap-3">
        {[0, 1, 2].map((i) => (
          <Skeleton key={i} className="h-20 w-full" />
        ))}
      </div>
    );
  }
  if (!data?.length) {
    return (
      <EmptyState
        icon={FileText}
        title="No documents yet"
        description="Open a job and click Prepare application to draft a tailored resume (and a cover letter if the job asks for one)."
        action={
          <Link href="/jobs?sort=best_fit" className="text-sm font-medium text-primary hover:underline">
            Find a job to apply to
          </Link>
        }
      />
    );
  }

  return (
    <ul className="flex flex-col gap-3" aria-label="Documents by job">
      {groupByJob(data).map((group) => (
        <li key={group.job_id}>
          <Link
            href={`/documents/${encodeURIComponent(group.job_id)}`}
            className="group flex flex-col gap-3 rounded-xl border bg-card p-4 transition-colors hover:border-primary/40 sm:flex-row sm:items-center"
          >
            <div className="min-w-0 flex-1">
              <p className="truncate font-medium">{group.job_title}</p>
              <p className="truncate text-sm text-muted-foreground">
                {group.company ?? "Company not listed"} · updated {timeAgo(group.updated_at)}
              </p>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              {(["resume", "cover_letter"] as const).map((kind) => {
                const doc = group.docs[kind];
                if (!doc) return null;
                return (
                  <Badge key={kind} tone={STATUS[doc.status].tone}>
                    {KIND_LABEL[kind]}: {STATUS[doc.status].label}
                    {doc.status === "ready" && doc.open_errors > 0 ? ` (${doc.open_errors})` : ""}
                  </Badge>
                );
              })}
              <ArrowRight className="size-4 text-muted-foreground transition-transform group-hover:translate-x-0.5" aria-hidden />
            </div>
          </Link>
        </li>
      ))}
    </ul>
  );
}
