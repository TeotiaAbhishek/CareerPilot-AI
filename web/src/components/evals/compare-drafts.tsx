"use client";

import { ExternalLink, FileText, Loader2, Play } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import {
  type CompareBatch,
  type CompareCell,
  type JobPage,
  compareDraftUrl,
  errorMessage,
  startCompare,
} from "@/lib/api";
import { dollars, rounded } from "@/lib/scoring";

const MAX_JOBS = 5;
const MAX_MODELS = 4;
const POLL_MS = 3000;
/** Picked first, when they're available. */
const SUGGESTED = ["claude-sonnet-5-5", "gpt-6.1-sol"];

/**
 * Tailored resumes for the same jobs from each model, side by side: the
 * checker's flags (claims your resume doesn't back up), length, time and cost.
 */
export function CompareDrafts({ models, draftingModel }: { models: string[]; draftingModel: string }) {
  const batch = useApi<CompareBatch | null>("/api/compare/drafts");
  const jobs = useApi<JobPage>("/api/jobs?sort=best_fit&page_size=20");
  const [pickedJobs, setPickedJobs] = useState<string[] | null>(null);
  const [pickedModels, setPickedModels] = useState<string[] | null>(null);
  const [starting, setStarting] = useState(false);

  const running = batch.data?.running ?? false;
  const { reload } = batch;
  useEffect(() => {
    if (!running) return;
    const timer = setInterval(reload, POLL_MS);
    return () => clearInterval(timer);
  }, [running, reload]);

  // Until you pick, the 3 best-fitting jobs and up to 3 models.
  const jobChoices = jobs.data?.items ?? [];
  const chosenJobs = pickedJobs ?? jobChoices.slice(0, 3).map((j) => j.id);
  const defaultModels = [draftingModel, ...SUGGESTED].filter((m, i, all) => models.includes(m) && all.indexOf(m) === i);
  const chosenModels = pickedModels ?? defaultModels.slice(0, 3);

  function toggle(list: string[], value: string, max: number): string[] {
    if (list.includes(value)) return list.filter((v) => v !== value);
    return list.length >= max ? list : [...list, value];
  }

  async function run() {
    setStarting(true);
    try {
      const started = await startCompare(chosenJobs, chosenModels);
      batch.mutate(() => started);
      toast.success(`Drafting ${started.cells.length} resumes`);
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setStarting(false);
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Compare tailored resumes</CardTitle>
        <CardDescription>
          Each model drafts a resume for the same jobs. Fewer flags means fewer claims your resume doesn&apos;t
          back up. These drafts are only for comparing: they never reach your Documents.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        <div className="grid gap-4 lg:grid-cols-2">
          <fieldset className="flex flex-col gap-1.5 text-sm">
            <legend className="mb-1.5 font-medium">
              Jobs <span className="font-normal text-muted-foreground">(up to {MAX_JOBS})</span>
            </legend>
            {jobs.error ? (
              <p className="text-danger">{jobs.error}</p>
            ) : !jobs.data ? (
              <Skeleton className="h-24 w-full" />
            ) : jobChoices.length === 0 ? (
              <p className="text-muted-foreground">No scored jobs yet.</p>
            ) : (
              <div className="flex max-h-52 flex-col gap-1 overflow-y-auto pr-1">
                {jobChoices.map((job) => (
                  <label key={job.id} className="flex items-start gap-2">
                    <input
                      type="checkbox"
                      className="mt-1"
                      checked={chosenJobs.includes(job.id)}
                      onChange={() => setPickedJobs(toggle(chosenJobs, job.id, MAX_JOBS))}
                    />
                    <span className="min-w-0">
                      {job.title}
                      <span className="text-muted-foreground">
                        {job.company ? ` · ${job.company}` : ""}
                        {job.score !== null ? ` · ${job.score}` : ""}
                      </span>
                    </span>
                  </label>
                ))}
              </div>
            )}
          </fieldset>

          <fieldset className="flex flex-col gap-1.5 text-sm">
            <legend className="mb-1.5 font-medium">
              Models <span className="font-normal text-muted-foreground">(up to {MAX_MODELS})</span>
            </legend>
            <div className="flex flex-col gap-1">
              {models.map((m) => (
                <label key={m} className="flex items-center gap-2">
                  <input
                    type="checkbox"
                    checked={chosenModels.includes(m)}
                    onChange={() => setPickedModels(toggle(chosenModels, m, MAX_MODELS))}
                  />
                  <span className="font-mono text-xs">{m}</span>
                </label>
              ))}
            </div>
            <Button
              className="mt-2 self-start"
              onClick={run}
              disabled={starting || running || chosenJobs.length === 0 || chosenModels.length === 0}
            >
              {starting || running ? <Loader2 className="animate-spin" /> : <Play />}
              {running ? "Drafting…" : `Draft ${chosenJobs.length * chosenModels.length} resumes`}
            </Button>
          </fieldset>
        </div>

        {batch.error ? <p className="text-sm text-danger">{batch.error}</p> : null}
        {batch.data ? <Results batch={batch.data} /> : null}
      </CardContent>
    </Card>
  );
}

function Results({ batch }: { batch: CompareBatch }) {
  const cell = (jobId: string, model: string) =>
    batch.cells.find((c) => c.job_id === jobId && c.model === model);

  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-[36rem] text-left text-sm">
        <thead className="text-xs text-muted-foreground">
          <tr>
            <th className="py-2 pr-4 font-medium">Job</th>
            {batch.models.map((m) => (
              <th key={m} className="py-2 pr-4 font-mono font-medium">
                {m}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y">
          <tr className="bg-muted/50">
            <td className="py-2.5 pr-4 font-medium">Average</td>
            {batch.summary.map((s) => (
              <td key={s.model} className="py-2.5 pr-4 align-top tabular-nums">
                <div className="font-medium">
                  {s.avg_flags === null ? "–" : `${s.avg_flags.toFixed(1)} flags`}
                </div>
                <div className="text-xs text-muted-foreground">
                  {rounded(s.avg_words)} words · {s.avg_seconds === null ? "–" : `${s.avg_seconds.toFixed(0)} s`} ·{" "}
                  {dollars(s.total_cost_usd)} total
                  {s.failed > 0 ? ` · ${s.failed} failed` : ""}
                </div>
              </td>
            ))}
          </tr>
          {batch.jobs.map((job) => (
            <tr key={job.id}>
              <td className="py-2.5 pr-4 align-top">
                <div className="font-medium">{job.title}</div>
                <div className="text-xs text-muted-foreground">{job.company}</div>
              </td>
              {batch.models.map((m) => (
                <td key={m} className="py-2.5 pr-4 align-top">
                  <CellView cell={cell(job.id, m)} />
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function CellView({ cell }: { cell: CompareCell | undefined }) {
  if (!cell) return <span className="text-muted-foreground">–</span>;
  if (cell.status === "queued") return <Badge>Waiting</Badge>;
  if (cell.status === "drafting")
    return (
      <Badge tone="primary">
        <Loader2 className="size-3 animate-spin" aria-hidden /> Drafting
      </Badge>
    );
  if (cell.status === "failed")
    return (
      <div className="flex flex-col gap-1">
        <Badge tone="danger" className="self-start">
          Failed
        </Badge>
        <span className="line-clamp-3 text-xs text-muted-foreground" title={cell.error ?? undefined}>
          {cell.error}
        </span>
      </div>
    );
  return (
    <div className="flex flex-col gap-1">
      <Badge tone={cell.flags === 0 ? "success" : "warning"} className="self-start tabular-nums">
        {cell.flags === 1 ? "1 flag" : `${cell.flags} flags`}
      </Badge>
      <span className="text-xs text-muted-foreground tabular-nums">
        {cell.words ?? "–"} words · {cell.duration_ms === null ? "–" : `${Math.round(cell.duration_ms / 1000)} s`} ·{" "}
        {cell.cost_usd === null ? "local" : dollars(cell.cost_usd)}
      </span>
      <a
        href={compareDraftUrl(cell.id)}
        target="_blank"
        rel="noreferrer"
        className="flex items-center gap-1 text-xs text-primary underline-offset-4 hover:underline"
      >
        <FileText className="size-3" aria-hidden /> Read <ExternalLink className="size-3" aria-hidden />
      </a>
    </div>
  );
}
