"use client";

import { ArrowRight, Bot, Loader2, Send } from "lucide-react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import { type Application, type JobDocuments, errorMessage, startApplication } from "@/lib/api";
import { STATUS, isActive, isClosed } from "@/lib/applications";
import { timeAgo } from "@/lib/format";

const POLL_MS = 4000;

/** Apply on a job: the agent fills in the form and stops before Submit. */
export function ApplyPanel({ jobId, applyType }: { jobId: string; applyType: string | null }) {
  const router = useRouter();
  const runs = useApi<Application[]>(`/api/applications?job_id=${encodeURIComponent(jobId)}`);
  const docs = useApi<JobDocuments>(`/api/jobs/${encodeURIComponent(jobId)}/documents`);
  const [busy, setBusy] = useState(false);

  const latest = runs.data?.[0];
  const open = latest && !isClosed(latest.status) ? latest : undefined;
  const reload = runs.reload;
  const reloadDocs = docs.reload;

  // While the agent works on it (or a draft is on its way), check every few seconds.
  const watching = (open && isActive(open.status)) || docs.data?.resume?.status === "drafting";
  useEffect(() => {
    if (!watching) return;
    const timer = setInterval(() => {
      reload();
      reloadDocs();
    }, POLL_MS);
    return () => clearInterval(timer);
  }, [watching, reload, reloadDocs]);

  async function apply() {
    setBusy(true);
    try {
      const run = await startApplication(jobId);
      toast.success("Queued. The agent opens the form in its browser window.");
      router.push(`/applications/${run.id}`);
    } catch (e) {
      toast.error(errorMessage(e));
      setBusy(false);
    }
  }

  const approved = docs.data?.resume?.status === "approved";
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
      ) : (
        <div className="mt-3 flex flex-col gap-3">
          <p className="text-sm text-muted-foreground">
            The agent opens {applyType === "Quick apply" ? "SEEK Quick Apply" : "the application form"} in its own
            browser window and fills it in from your profile, answer bank and approved documents. It stops before
            Submit, so you check everything and click Submit yourself.
          </p>
          <div className="flex flex-wrap items-center gap-3">
            <Button disabled={busy || !approved} onClick={apply}>
              {busy ? <Loader2 className="animate-spin" /> : <Send />} Apply with the agent
            </Button>
            {approved ? (
              <span className="text-xs text-muted-foreground">
                Uploads your approved resume{letterApproved ? " and cover letter" : ""}.
              </span>
            ) : (
              <span className="text-xs text-muted-foreground">Approve a resume above first.</span>
            )}
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
