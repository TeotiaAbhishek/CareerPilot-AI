"use client";

import {
  ArrowLeft,
  CheckCircle2,
  Download,
  FileText,
  Loader2,
  Printer,
  RotateCcw,
  Save,
  Sparkles,
  TriangleAlert,
} from "lucide-react";
import Link from "next/link";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { type ReactNode, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Textarea } from "@/components/ui/textarea";
import { useApi } from "@/hooks/use-api";
import {
  type DocumentKind,
  type DocumentView,
  type JobDetail,
  type JobDocuments,
  type LetterDraft,
  type PrepareInput,
  type ResumeDraft,
  apiGet,
  approveDocument,
  docxUrl,
  errorMessage,
  prepareDocuments,
  saveDocument,
} from "@/lib/api";
import { KIND_LABEL, STATUS, isPending, openErrors } from "@/lib/documents";
import { timeAgo } from "@/lib/format";
import { cn } from "@/lib/utils";

import { LetterDraftEditor, ResumeDraftEditor } from "./draft-editors";

const POLL_MS = 5000;
const KINDS: DocumentKind[] = ["resume", "cover_letter"];

/** The review screen for one job: the ad on the left, your draft on the right. */
export function DocumentReview({ jobId }: { jobId: string }) {
  const searchParams = useSearchParams();
  const router = useRouter();
  const pathname = usePathname();
  const tab: DocumentKind = searchParams.get("tab") === "cover_letter" ? "cover_letter" : "resume";

  const path = `/api/jobs/${encodeURIComponent(jobId)}/documents`;
  const { data, error, mutate } = useApi<JobDocuments>(path);
  const { data: job } = useApi<JobDetail>(`/api/jobs/${encodeURIComponent(jobId)}`);

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
    try {
      const fresh = await prepareDocuments(jobId, input);
      mutate(() => fresh);
      toast.success(message);
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  const document = data?.[tab] ?? null;

  return (
    <>
      <Link
        href={`/jobs/${encodeURIComponent(jobId)}`}
        className="mb-4 inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
      >
        <ArrowLeft className="size-4" aria-hidden /> Back to the job
      </Link>

      <div className="mb-6">
        <h1 className="text-2xl font-semibold tracking-tight">
          {data?.resume?.job_title ?? data?.cover_letter?.job_title ?? job?.title ?? "Documents"}
        </h1>
        <p className="mt-1 text-sm text-muted-foreground">
          {(data?.resume?.company ?? job?.company) || "Company not listed"} · tailored documents
        </p>
      </div>

      <div role="tablist" aria-label="Documents" className="mb-6 flex gap-1 overflow-x-auto border-b">
        {KINDS.map((kind) => {
          const doc = data?.[kind];
          return (
            <button
              key={kind}
              type="button"
              role="tab"
              aria-selected={tab === kind}
              onClick={() =>
                router.replace(kind === "resume" ? pathname : `${pathname}?tab=${kind}`, { scroll: false })
              }
              className={cn(
                "-mb-px flex items-center gap-2 border-b-2 px-3 py-2 text-sm font-medium whitespace-nowrap transition-colors",
                tab === kind
                  ? "border-primary text-foreground"
                  : "border-transparent text-muted-foreground hover:text-foreground",
              )}
            >
              <FileText className="size-4" aria-hidden />
              {KIND_LABEL[kind]}
              {doc ? (
                <span
                  className={cn(
                    "size-2 rounded-full",
                    doc.status === "approved" ? "bg-success" : doc.status === "failed" ? "bg-danger" : "bg-warning",
                  )}
                  aria-label={STATUS[doc.status].label}
                />
              ) : null}
            </button>
          );
        })}
      </div>

      {error ? (
        <div className="rounded-xl border border-danger/30 bg-danger/5 p-6 text-sm text-danger">{error}</div>
      ) : !data ? (
        <Skeleton className="h-96 w-full" />
      ) : (
        <div className="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
          <JobAd job={job} />
          <DocumentPane
            // A fresh editor whenever the saved document changes (new draft, save, approval).
            key={document ? `${document.id}:${document.updated_at}` : `none:${tab}`}
            kind={tab}
            jobId={jobId}
            document={document}
            queueAhead={data.queue_ahead}
            onPrepare={prepare}
            onSaved={(saved) => mutate((d) => ({ ...d, [saved.kind]: saved }))}
          />
        </div>
      )}
    </>
  );
}

function JobAd({ job }: { job: JobDetail | undefined }) {
  const body = !job ? (
    <Skeleton className="h-64 w-full" />
  ) : job.description_html ? (
    <div
      className="prose prose-sm max-w-none dark:prose-invert prose-headings:font-semibold prose-a:text-primary"
      // Safe: the backend cleans this HTML with ammonia before storing it.
      dangerouslySetInnerHTML={{ __html: job.description_html }}
    />
  ) : (
    <p className="text-sm whitespace-pre-line">{job.description_text ?? "No description."}</p>
  );

  return (
    <aside className="lg:sticky lg:top-20 lg:max-h-[calc(100dvh-6rem)] lg:overflow-y-auto">
      <details className="rounded-xl border bg-card" open>
        <summary className="cursor-pointer list-none border-b p-4 text-sm font-semibold lg:cursor-default">
          The job ad
          <span className="ml-2 text-xs font-normal text-muted-foreground lg:hidden">(tap to hide)</span>
        </summary>
        <div className="p-4">{body}</div>
      </details>
    </aside>
  );
}

function DocumentPane({
  kind,
  jobId,
  document,
  queueAhead,
  onPrepare,
  onSaved,
}: {
  kind: DocumentKind;
  jobId: string;
  document: DocumentView | null;
  queueAhead: number;
  onPrepare: (input: PrepareInput, message: string) => Promise<void>;
  onSaved: (document: DocumentView) => void;
}) {
  const only = { resume: kind === "resume", cover_letter: kind === "cover_letter" };
  const label = KIND_LABEL[kind];

  if (!document) {
    return (
      <Panel>
        <div className="flex flex-col items-center gap-3 py-10 text-center">
          <FileText className="size-8 text-muted-foreground" aria-hidden />
          <p className="font-medium">No {label.toLowerCase()} yet</p>
          <p className="max-w-sm text-sm text-muted-foreground">
            {kind === "cover_letter"
              ? "Only add one if the job asks for a cover letter."
              : "Draft a resume tailored to this job."}
          </p>
          <Button onClick={() => onPrepare(only, `Drafting a ${label.toLowerCase()}`)}>
            <Sparkles /> Write {label.toLowerCase()}
          </Button>
        </div>
      </Panel>
    );
  }

  if (isPending(document.status)) {
    return (
      <Panel>
        <div className="flex flex-col items-center gap-3 py-10 text-center" aria-live="polite">
          <Loader2 className="size-8 animate-spin text-primary" aria-hidden />
          <p className="font-medium">
            {document.status === "queued"
              ? queueAhead > 0
                ? `Waiting: ${queueAhead} ahead in the queue`
                : "Starting soon"
              : `Writing your ${label.toLowerCase()}`}
          </p>
          <p className="max-w-sm text-sm text-muted-foreground">
            Your local model takes 1-3 minutes per document. You can leave this page: it keeps going.
          </p>
        </div>
      </Panel>
    );
  }

  if (document.status === "failed" || !document.content) {
    return (
      <Panel>
        <div className="flex flex-col items-center gap-3 py-10 text-center">
          <TriangleAlert className="size-8 text-danger" aria-hidden />
          <p className="font-medium">Drafting failed</p>
          <p className="max-w-md font-mono text-xs text-muted-foreground">{document.error}</p>
          <Button variant="outline" onClick={() => onPrepare(only, "Trying again")}>
            <RotateCcw /> Try again
          </Button>
        </div>
      </Panel>
    );
  }

  return <Editor document={document} jobId={jobId} onPrepare={onPrepare} onSaved={onSaved} />;
}

function Panel({ children }: { children: ReactNode }) {
  return <div className="rounded-xl border bg-card p-6">{children}</div>;
}

function Editor({
  document,
  jobId,
  onPrepare,
  onSaved,
}: {
  document: DocumentView;
  jobId: string;
  onPrepare: (input: PrepareInput, message: string) => Promise<void>;
  onSaved: (document: DocumentView) => void;
}) {
  const [draft, setDraft] = useState(document.content as ResumeDraft | LetterDraft);
  const [dismissed, setDismissed] = useState(document.dismissed_flags);
  const [busy, setBusy] = useState<null | "save" | "approve">(null);
  const [regenerating, setRegenerating] = useState(false);
  const [note, setNote] = useState(document.note ?? "");

  const dirty =
    JSON.stringify(draft) !== JSON.stringify(document.content) ||
    JSON.stringify(dismissed) !== JSON.stringify(document.dismissed_flags);
  const open = openErrors(document.flags, dismissed);
  const evidence = useMemo(() => new Map(document.evidence.map((e) => [e.id, e.text])), [document.evidence]);

  // Warn before closing the tab with unsaved edits.
  useEffect(() => {
    if (!dirty) return;
    const warn = (event: BeforeUnloadEvent) => event.preventDefault();
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [dirty]);

  const ctx = {
    flags: document.flags,
    dismissed,
    evidence,
    disabled: busy !== null,
    onDismiss: (key: string, on: boolean) =>
      setDismissed((d) => (on ? [...d.filter((k) => k !== key), key] : d.filter((k) => k !== key))),
  };

  async function save(): Promise<DocumentView | null> {
    setBusy("save");
    try {
      const saved = await saveDocument(document.id, draft, dismissed);
      toast.success(saved.open_errors > 0 ? `Saved. ${saved.open_errors} line(s) still to check.` : "Saved");
      onSaved(saved);
      return saved;
    } catch (e) {
      toast.error(errorMessage(e));
      return null;
    } finally {
      setBusy(null);
    }
  }

  async function approve() {
    let current = document;
    if (dirty) {
      const saved = await save();
      if (!saved) return;
      current = saved;
      if (saved.open_errors > 0) return;
    }
    setBusy("approve");
    try {
      const approved = await approveDocument(current.id);
      toast.success(`${KIND_LABEL[document.kind]} approved. The apply agents may now use it.`);
      onSaved(approved);
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(null);
    }
  }

  async function regenerate() {
    setBusy("save");
    await onPrepare(
      { resume: document.kind === "resume", cover_letter: document.kind === "cover_letter", note },
      "Writing a new draft. The current one stays in your history.",
    );
    setBusy(null);
  }

  const status = STATUS[document.status];
  const printHref = `/documents/${encodeURIComponent(jobId)}/print${document.kind === "cover_letter" ? "?tab=cover_letter" : ""}`;

  return (
    <div className="rounded-xl border bg-card">
      <div className="flex flex-col gap-3 border-b p-4">
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <Badge tone={status.tone}>{status.label}</Badge>
          {open > 0 ? (
            <Badge tone="danger">
              {open} line{open === 1 ? "" : "s"} to check
            </Badge>
          ) : (
            <Badge tone="success">
              <CheckCircle2 className="size-3" aria-hidden /> All lines backed by your resume
            </Badge>
          )}
          <span className="text-xs text-muted-foreground">
            {document.model} · {Math.round((document.duration_ms ?? 0) / 1000)} s
            {document.edited ? " · edited" : ""}
            {document.previous_drafts > 0 ? ` · ${document.previous_drafts} earlier draft(s)` : ""} ·{" "}
            {timeAgo(document.updated_at)}
          </span>
        </div>

        <div className="flex flex-wrap gap-2">
          <Button
            onClick={approve}
            disabled={busy !== null || open > 0 || (!dirty && document.status === "approved")}
            title={open > 0 ? "Fix or accept the flagged lines first" : undefined}
          >
            {busy === "approve" ? <Loader2 className="animate-spin" /> : <CheckCircle2 />}
            {document.status === "approved" && !dirty ? "Approved" : dirty ? "Save and approve" : "Approve"}
          </Button>
          <Button variant="outline" onClick={save} disabled={busy !== null || !dirty}>
            {busy === "save" ? <Loader2 className="animate-spin" /> : <Save />} Save
          </Button>
          <a
            href={dirty ? undefined : docxUrl(document.id)}
            aria-disabled={dirty}
            title={dirty ? "Save first" : "Download as a Word file"}
            className={cn(buttonVariants({ variant: "outline" }), dirty && "pointer-events-none opacity-50")}
          >
            <Download /> Word
          </a>
          <Link
            href={printHref}
            target="_blank"
            aria-disabled={dirty}
            title={dirty ? "Save first" : "Open a print view to save as PDF"}
            className={cn(buttonVariants({ variant: "outline" }), dirty && "pointer-events-none opacity-50")}
          >
            <Printer /> PDF
          </Link>
          <Button variant="ghost" onClick={() => setRegenerating((r) => !r)} aria-expanded={regenerating}>
            <RotateCcw /> Regenerate
          </Button>
        </div>

        {regenerating ? (
          <div className="flex flex-col gap-2 rounded-lg bg-muted p-3">
            <label htmlFor="note" className="text-sm font-medium">
              Anything the next draft should do differently? (optional)
            </label>
            <Textarea
              id="note"
              rows={2}
              maxLength={500}
              value={note}
              placeholder="e.g. Lead with the Power BI work, keep it to 3 bullets per job"
              onChange={(e) => setNote(e.target.value)}
            />
            <div>
              <Button size="sm" onClick={regenerate} disabled={busy !== null}>
                <Sparkles /> Write a new draft
              </Button>
            </div>
          </div>
        ) : null}

        {dirty ? (
          <p className="text-xs text-muted-foreground">
            Unsaved changes. Saving runs the fact check again.
          </p>
        ) : null}
      </div>

      <div className="p-4 sm:p-6">
        {document.kind === "resume" ? (
          <ResumeDraftEditor draft={draft as ResumeDraft} ctx={ctx} onChange={setDraft} />
        ) : (
          <LetterDraftEditor draft={draft as LetterDraft} ctx={ctx} onChange={setDraft} />
        )}
      </div>
    </div>
  );
}
