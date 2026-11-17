"use client";

import { FlaskConical, Loader2, Play, Tags } from "lucide-react";
import Link from "next/link";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { PageHeader } from "@/components/ui/page-header";

import { CompareDrafts } from "./compare-drafts";
import { Select } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import { type EvalReport, type ScoringStatus, type Settings, errorMessage, runEval } from "@/lib/api";
import { LABEL_TONE, LABELS, dollars, percent, rounded } from "@/lib/scoring";
import { cn } from "@/lib/utils";

/** Labels needed before the numbers mean much. */
const LABEL_GOAL = 40;
const POLL_MS = 5000;
/** One model per provider: local, Claude and OpenAI, compared on the same jobs. */
const ALL_THREE = ["qwen3:14b", "claude-haiku-5-5", "gpt-6-luna"];

/**
 * Evals: how often each model (local, Claude or OpenAI) agrees with your
 * Apply / Maybe / No labels on recent jobs, what a score costs, the jobs where
 * the active model disagrees, and tailored resumes from each model side by side.
 */
export function EvalsView() {
  const report = useApi<EvalReport>("/api/evals");
  const status = useApi<ScoringStatus>("/api/scoring/status");
  const settings = useApi<Settings>("/api/settings");
  const [model, setModel] = useState("");
  const [running, setRunning] = useState(false);

  // While an eval run is in the queue, refresh both every few seconds.
  const busy = (status.data?.queue.requested ?? 0) > 0;
  const { reload: reloadReport } = report;
  const { reload: reloadStatus } = status;
  useEffect(() => {
    if (!busy) return;
    const timer = setInterval(() => {
      reloadReport();
      reloadStatus();
    }, POLL_MS);
    return () => clearInterval(timer);
  }, [busy, reloadReport, reloadStatus]);

  const models = status.data?.ollama.models ?? [];
  const chosen = model || models.find((m) => m !== status.data?.model) || models[0] || "";
  // Only the ones you can run: Claude and OpenAI need their key.
  const allThree = ALL_THREE.filter((m) => models.includes(m));

  async function run(picked: string[]) {
    if (picked.length === 0) return;
    setRunning(true);
    try {
      const { queued } = await runEval(picked);
      const names = picked.join(", ");
      toast.success(
        queued === 0 ? `${names}: every recent labelled job is already scored` : `Queued ${queued} scores for ${names}`,
      );
      reloadStatus();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setRunning(false);
    }
  }

  return (
    <>
      <PageHeader
        title="Evals"
        description="How often each model agrees with your own Apply / Maybe / No labels, what it costs, and how well it tailors a resume."
      />

      {report.error ? (
        <EmptyState icon={FlaskConical} title="Couldn't load evals" description={report.error} />
      ) : !report.data ? (
        <Skeleton className="h-96 w-full" />
      ) : (
        <div className="flex flex-col gap-6">
          <div className="grid gap-4 lg:grid-cols-2">
            <LabelProgress report={report.data} />

            <Card>
              <CardHeader>
                <CardTitle>Run an eval</CardTitle>
                <CardDescription>
                  Scores every labelled job from the last {report.data.window} with another model, so you can
                  compare it here: a local one, or Claude or OpenAI once their key is set. Or run all three
                  providers on the same jobs at once.
                </CardDescription>
              </CardHeader>
              <CardContent className="flex flex-wrap items-center gap-2">
                <Select
                  aria-label="Model to evaluate"
                  value={chosen}
                  onChange={(e) => setModel(e.target.value)}
                  disabled={models.length === 0}
                  className="w-52"
                >
                  {models.length === 0 ? <option value="">No models</option> : null}
                  {models.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </Select>
                <Button onClick={() => run([chosen])} disabled={!chosen || running || report.data.labels.total === 0}>
                  {running ? <Loader2 className="animate-spin" /> : <Play />} Run eval
                </Button>
                <Button
                  variant="outline"
                  onClick={() => run(allThree)}
                  disabled={allThree.length < 2 || running || report.data.labels.total === 0}
                  title={allThree.join(", ")}
                >
                  <Play /> Run all three
                </Button>
                {busy ? (
                  <span className="flex items-center gap-2 text-sm text-muted-foreground">
                    <Loader2 className="size-4 animate-spin" aria-hidden />
                    {status.data?.queue.requested} jobs left
                  </span>
                ) : null}
              </CardContent>
            </Card>
          </div>

          <ResultsTable report={report.data} />
          <Disagreements report={report.data} />
          {status.data && settings.data ? (
            <CompareDrafts models={models} draftingModel={settings.data.drafting_model} />
          ) : null}
        </div>
      )}
    </>
  );
}

function LabelProgress({ report }: { report: EvalReport }) {
  const { labels } = report;
  const progress = Math.min(1, labels.total / LABEL_GOAL);

  return (
    <Card>
      <CardHeader>
        <CardTitle>Your labels</CardTitle>
        <CardDescription>
          {labels.total < LABEL_GOAL
            ? `Label ${LABEL_GOAL - labels.total} more jobs for numbers you can trust. `
            : "Enough labels to compare models. "}
          Only jobs from the last {report.window} count.
          Open a job and press <kbd className="font-mono">a</kbd>, <kbd className="font-mono">m</kbd>{" "}
          or <kbd className="font-mono">n</kbd>.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="mb-3 h-1.5 overflow-hidden rounded-full bg-muted">
          <div className="h-full rounded-full bg-primary transition-all" style={{ width: `${progress * 100}%` }} />
        </div>
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <span className="font-medium tabular-nums">
            {labels.total} / {LABEL_GOAL}
          </span>
          {LABELS.map((l) => (
            <Badge key={l.value} tone={LABEL_TONE[l.value]} className="tabular-nums">
              {l.text} {labels[l.value]}
            </Badge>
          ))}
          <Link href="/jobs?posted=7&relevant=0&sort=best_fit" className="ml-auto text-primary underline-offset-4 hover:underline">
            Label jobs
          </Link>
        </div>
      </CardContent>
    </Card>
  );
}

function ResultsTable({ report }: { report: EvalReport }) {
  if (report.rows.length === 0) {
    return (
      <EmptyState
        icon={Tags}
        title="No results yet"
        description="Label some jobs. Results appear once a model has scored a labelled job."
      />
    );
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Results</CardTitle>
        <CardDescription>
          A model says “relevant” when the score is {report.threshold}+ with no hard blockers.
          Agreement, precision and recall compare that with your Apply and No labels (Maybe is left out).
        </CardDescription>
      </CardHeader>
      <CardContent className="overflow-x-auto">
        <table className="w-full text-left text-sm">
          <thead className="text-xs text-muted-foreground">
            <tr>
              <th className="py-2 pr-4 font-medium">Model</th>
              <th className="py-2 pr-4 text-right font-medium">Scored</th>
              <th className="py-2 pr-4 text-right font-medium" title="Share of Apply/No jobs where the model agrees with you">
                Agreement
              </th>
              <th className="py-2 pr-4 text-right font-medium" title="Of jobs it calls relevant, how many you'd apply to">
                Precision
              </th>
              <th className="py-2 pr-4 text-right font-medium" title="Of jobs you'd apply to, how many it calls relevant">
                Recall
              </th>
              <th className="py-2 pr-4 text-right font-medium">Avg Apply</th>
              <th className="py-2 pr-4 text-right font-medium">Avg Maybe</th>
              <th className="py-2 pr-4 text-right font-medium">Avg No</th>
              <th className="py-2 pr-4 text-right font-medium" title="Average cost of one score">
                Cost
              </th>
              <th className="py-2 text-right font-medium" title="Average time for one score">
                Time
              </th>
            </tr>
          </thead>
          <tbody className="divide-y">
            {report.rows.map((row) => (
              <tr key={`${row.model}-${row.prompt_version}`} className={cn(row.active && "bg-primary/5")}>
                <td className="py-2.5 pr-4">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-medium">{row.model}</span>
                    {row.active ? <Badge tone="primary">Active</Badge> : null}
                  </div>
                  <span className="text-xs text-muted-foreground">
                    prompt {row.prompt_version}
                    {row.capped > 0 ? ` · ${row.capped} capped for seniority` : ""}
                  </span>
                </td>
                <td className="py-2.5 pr-4 text-right tabular-nums">{row.metrics.scored}</td>
                <td className="py-2.5 pr-4 text-right font-medium tabular-nums">{percent(row.metrics.agreement)}</td>
                <td className="py-2.5 pr-4 text-right tabular-nums">{percent(row.metrics.precision)}</td>
                <td className="py-2.5 pr-4 text-right tabular-nums">{percent(row.metrics.recall)}</td>
                <td className="py-2.5 pr-4 text-right tabular-nums">{rounded(row.metrics.mean_score_apply)}</td>
                <td className="py-2.5 pr-4 text-right tabular-nums">{rounded(row.metrics.mean_score_maybe)}</td>
                <td className="py-2.5 pr-4 text-right tabular-nums">{rounded(row.metrics.mean_score_no)}</td>
                <td className="py-2.5 pr-4 text-right tabular-nums">
                  {row.avg_cost_usd === null ? "free" : dollars(row.avg_cost_usd)}
                </td>
                <td className="py-2.5 text-right tabular-nums">
                  {row.avg_seconds === null ? "–" : `${row.avg_seconds.toFixed(1)} s`}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </CardContent>
    </Card>
  );
}

function Disagreements({ report }: { report: EvalReport }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Where {report.active_model} disagrees with you</CardTitle>
        <CardDescription>
          Biggest misses first. These are the jobs to read when improving the prompt.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {report.disagreements.length === 0 ? (
          <p className="text-sm text-muted-foreground">No disagreements so far.</p>
        ) : (
          <ul className="divide-y">
            {report.disagreements.map((d) => (
              <li key={d.job_id} className="flex flex-col gap-1.5 py-3 sm:flex-row sm:items-start sm:gap-4">
                <div className="flex shrink-0 items-center gap-2">
                  <Badge tone={LABEL_TONE[d.label]}>You: {LABELS.find((l) => l.value === d.label)?.text}</Badge>
                  <Badge tone={d.blockers > 0 ? "danger" : "neutral"} className="tabular-nums">
                    Model: {d.score}
                    {d.blockers > 0 ? " · blocked" : ""}
                  </Badge>
                </div>
                <div className="min-w-0">
                  <Link href={`/jobs/${encodeURIComponent(d.job_id)}`} className="font-medium hover:text-primary">
                    {d.title}
                  </Link>
                  <span className="text-muted-foreground">
                    {d.company ? ` · ${d.company}` : ""}
                  </span>
                  <p className="mt-0.5 text-sm text-muted-foreground">{d.summary}</p>
                </div>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}
