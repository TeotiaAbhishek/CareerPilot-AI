"use client";

import { ArrowRight, FileText, Loader2, RotateCcw, Sparkles } from "lucide-react";
import Link from "next/link";
import { type ReactNode, useEffect, useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import {
  type DocumentKind,
  type DocumentView,
  type JobDocuments,
  type PrepareInput,
  apiGet,
  errorMessage,
  prepareDocuments,
} from "@/lib/api";
import { KIND_LABEL, STATUS, isPending } from "@/lib/documents";
import { timeAgo } from "@/lib/format";

const POLL_MS = 5000;

/** "Prepare application" on a job, and the state of its tailored documents. */
export function PreparePanel({ jobId, canDraft }: { jobId: string; canDraft: boolean }) {
  const path = `/api/jobs/${encodeURIComponent(jobId)}/documents`;
  const { data, error, mutate } = useApi<JobDocuments>(path);
  const [coverLetter, setCoverLetter] = useState(false);
  const [busy, setBusy] = useState(false);

  // While a draft is on its way, check quietly every few seconds.
  const pending = isPending(data?.resume?.status) || isPending(data?.cover_letter?.status);
  useEffect(() => {
    if (!pending) return;
    const timer = setInterval(() => {
      apiGet<JobDocuments>(path, { cache: "no-store" }).then(
        (fresh) => mutate(() => fresh),
        () => {},
      );
    }, POLL_MS);
    return () => clearInterval(timer);
  }, [pending, path, mutate]);

  async function prepare(input: PrepareInput, message: string) {
    setBusy(true);
    try {
      const fresh = await prepareDocuments(jobId, input);
      mutate(() => fresh);
      toast.success(message);
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section aria-labelledby="documents-title" className="border-t p-6">
      <h3 id="documents-title" className="text-sm font-semibold">
        Application documents
      </h3>

      {error ? (
        <p className="mt-3 text-sm text-danger">{error}</p>
      ) : !data ? (
        <Skeleton className="mt-3 h-16 w-full" />
      ) : !data.resume && !data.cover_letter ? (
        <div className="mt-3 flex flex-col gap-3">
          <p className="text-sm text-muted-foreground">
            Draft a resume tailored to this job from your master resume. Every line cites the part of
            your resume it comes from, and you review it before anything is sent.
          </p>
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={coverLetter}
              onChange={(e) => setCoverLetter(e.target.checked)}
              className="size-4 accent-primary"
            />
            Also write a cover letter (only if the job asks for one)
          </label>
          <div>
            <Button
              disabled={busy || !canDraft}
              onClick={() =>
                prepare(
                  { resume: true, cover_letter: coverLetter },
                  coverLetter ? "Drafting your resume and cover letter" : "Drafting your resume",
                )
              }
            >
              {busy ? <Loader2 className="animate-spin" /> : <Sparkles />} Prepare application
            </Button>
            {!canDraft ? (
              <p className="mt-2 text-xs text-muted-foreground">
                This job has no description yet, so there&apos;s nothing to tailor to.
              </p>
            ) : null}
          </div>
        </div>
      ) : (
        <ul className="mt-3 flex flex-col divide-y rounded-lg border">
          {(["resume", "cover_letter"] as const).map((kind) => (
            <DocumentRow
              key={kind}
              kind={kind}
              jobId={jobId}
              document={data[kind]}
              queueAhead={data.queue_ahead}
              busy={busy}
              onPrepare={prepare}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

function DocumentRow({
  kind,
  jobId,
  document,
  queueAhead,
  busy,
  onPrepare,
}: {
  kind: DocumentKind;
  jobId: string;
  document: DocumentView | null;
  queueAhead: number;
  busy: boolean;
  onPrepare: (input: PrepareInput, message: string) => void;
}) {
  const label = KIND_LABEL[kind];
  const only = { resume: kind === "resume", cover_letter: kind === "cover_letter" };
  const reviewHref = `/documents/${encodeURIComponent(jobId)}${kind === "cover_letter" ? "?tab=cover_letter" : ""}`;

  let detail: ReactNode;
  let action: ReactNode = null;
  if (!document) {
    detail = "Not requested";
    action = (
      <Button size="sm" variant="outline" disabled={busy} onClick={() => onPrepare(only, `Drafting a ${label.toLowerCase()}`)}>
        <Sparkles /> Add
      </Button>
    );
  } else if (document.status === "queued") {
    detail = queueAhead > 0 ? `Waiting: ${queueAhead} ahead in the queue` : "Starting soon";
  } else if (document.status === "drafting") {
    detail = (
      <span className="inline-flex items-center gap-1.5">
        <Loader2 className="size-3.5 animate-spin text-primary" aria-hidden />
        Writing (started {timeAgo(document.updated_at)}). This takes 1-3 minutes.
      </span>
    );
  } else if (document.status === "failed") {
    detail = <span className="text-danger">{document.error ?? "Drafting failed."}</span>;
    action = (
      <Button size="sm" variant="outline" disabled={busy} onClick={() => onPrepare(only, "Trying again")}>
        <RotateCcw /> Try again
      </Button>
    );
  } else {
    detail =
      document.status === "approved"
        ? `Approved ${timeAgo(document.approved_at)}`
        : document.open_errors > 0
          ? `${document.open_errors} line${document.open_errors === 1 ? "" : "s"} to check`
          : "Ready to approve";
    action = (
      <Link href={reviewHref} className={buttonVariants({ size: "sm", variant: "outline" })}>
        Review <ArrowRight />
      </Link>
    );
  }

  return (
    <li className="flex flex-wrap items-center gap-x-3 gap-y-2 p-3 text-sm">
      <FileText className="size-4 shrink-0 text-muted-foreground" aria-hidden />
      <span className="font-medium">{label}</span>
      {document ? <Badge tone={STATUS[document.status].tone}>{STATUS[document.status].label}</Badge> : null}
      <span className="min-w-0 flex-1 text-muted-foreground">{detail}</span>
      {action}
    </li>
  );
}
