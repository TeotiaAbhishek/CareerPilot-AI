"use client";

import { ArrowRight, Bot, Loader2, Send } from "lucide-react";
import Link from "next/link";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import { type Application, type Card, type JobDocuments, errorMessage, oneClickApply } from "@/lib/api";
import { STATUS, isActive, isClosed } from "@/lib/applications";
import { timeAgo } from "@/lib/format";
import { STARTED, columnInfo } from "@/lib/tracker";

const POLL_MS = 4000;

/**
 * Apply on a job in one click: draft a resume if there isn't one, approve it
 * if the checker found nothing, then the agent fills in the form and stops
 * before Submit.
 */
export function ApplyPanel({
  jobId,
  applyType,
  onStarted,
}: {
  jobId: string;
  applyType: string | null;
  /** Called after one-click starts, so the page can show the new draft. */
  onStarted?: () => void;
}) {
  const id = encodeURIComponent(jobId);
  const runs = useApi<Application[]>(`/api/applications?job_id=${id}`);
  const docs = useApi<JobDocuments>(`/api/jobs/${id}/documents`);
  // A job that isn't on the board yet answers 404: that just means "not started".
  const tracked = useApi<Card>(`/api/tracker/${id}`);
  const [busy, setBusy] = useState(false);

  const latest = runs.data?.[0];
  const open = latest && !isClosed(latest.status) ? latest : undefined;
  const card = tracked.error ? undefined : tracked.data;
  const { reload } = runs;
  const reloadDocs = docs.reload;
  const reloadCard = tracked.reload;

  // One-click is on its way (drafting, or waiting for your review) without a run yet.
  const pending = !open && card && (card.column === "applying" || card.column === "your_turn") ? card : undefined;

  // While the agent works on it, or one-click is waiting on a draft or your
  // review, check every few seconds.
  const resume = docs.data?.resume?.status;
  const watching = (open && isActive(open.status)) || resume === "queued" || resume === "drafting" || !!pending;
  useEffect(() => {
    if (!watching) return;
    const timer = setInterval(() => {
      reload();
      reloadDocs();
      reloadCard();
    }, POLL_MS);
    return () => clearInterval(timer);
  }, [watching, reload, reloadDocs, reloadCard]);

  async function apply() {
    setBusy(true);
    try {
      const { started } = await oneClickApply(jobId);
      toast.success(STARTED[started]);
      onStarted?.();
      reload();
      reloadDocs();
      reloadCard();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const approved = resume === "approved";
  const letterApproved = docs.data?.cover_letter?.status === "approved";

  return (
    <section aria-labelledby="apply-title" className="border-t p-6">
      <h3 id="apply-title" className="text-sm font-semibold">
        Apply
      </h3>

      {runs.error ? (
        <p className="mt-3 text-sm text-danger">{runs.error}</p>
      ) : !runs.data || !docs.data ? (
        <Skeleton className="mt-3 h-14 w-full" />
      ) : open ? (
        <div className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-2 rounded-lg border p-3 text-sm">
          {isActive(open.status) ? (
            <Loader2 className="size-4 animate-spin text-primary" aria-hidden />
          ) : (
            <Bot className="size-4 text-muted-foreground" aria-hidden />
          )}
          <Badge tone={STATUS[open.status].tone}>{STATUS[open.status].label}</Badge>
          <span className="min-w-0 flex-1 text-muted-foreground">
            {open.message ?? `Started ${timeAgo(open.created_at)}`}
          </span>
          <Link href={`/applications/${open.id}`} className={buttonVariants({ size: "sm", variant: "outline" })}>
            Open run <ArrowRight />
          </Link>
        </div>
      ) : pending ? (
        <div className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-2 rounded-lg border p-3 text-sm">
          {pending.column === "applying" ? (
            <Loader2 className="size-4 animate-spin text-primary" aria-hidden />
          ) : (
            <Bot className="size-4 text-muted-foreground" aria-hidden />
          )}
          <Badge tone={columnInfo(pending.column).tone}>{columnInfo(pending.column).label}</Badge>
          <span className="min-w-0 flex-1 text-muted-foreground">{pending.detail}</span>
          {pending.column === "your_turn" && resume === "approved" ? (
            <Button size="sm" disabled={busy} onClick={apply}>
              {busy ? <Loader2 className="animate-spin" /> : <Send />} Apply
            </Button>
          ) : resume === "ready" ? (
            <Link href={`/documents/${id}`} className={buttonVariants({ size: "sm", variant: "outline" })}>
              Review <ArrowRight />
            </Link>
          ) : null}
        </div>
      ) : (
        <div className="mt-3 flex flex-col gap-3">
          <p className="text-sm text-muted-foreground">
            The agent opens {applyType === "Quick apply" ? "SEEK Quick Apply" : "the application form"} in its own
            browser window and fills it in from your profile, answer bank and approved documents. It stops before
            Submit, so you check everything and click Submit yourself.
          </p>
          <div className="flex flex-wrap items-center gap-3">
            <Button disabled={busy} onClick={apply}>
              {busy ? <Loader2 className="animate-spin" /> : <Send />} Apply
            </Button>
            <span className="text-xs text-muted-foreground">
              {approved
                ? `Uploads your approved resume${letterApproved ? " and cover letter" : ""}.`
                : "Drafts a resume first. If the checker finds nothing to fix, it's approved and the agent starts."}
            </span>
          </div>
          {latest ? (
            <p className="text-xs text-muted-foreground">
              Last run: {STATUS[latest.status].label.toLowerCase()} {timeAgo(latest.updated_at)}.{" "}
              <Link href={`/applications/${latest.id}`} className="text-primary hover:underline">
                See what happened
              </Link>
            </p>
          ) : null}
        </div>
      )}
    </section>
  );
}
