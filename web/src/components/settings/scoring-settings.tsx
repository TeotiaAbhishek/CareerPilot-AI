"use client";

import { Cpu, Loader2, TriangleAlert } from "lucide-react";
import Link from "next/link";
import { type FormEvent, useEffect, useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { useApi } from "@/hooks/use-api";
import { type ScoringStatus, type SettingsPatch, errorMessage, updateSettings } from "@/lib/api";
import { timeAgo } from "@/lib/format";
import { cn } from "@/lib/utils";

const POLL_MS = 5000;

/** Which local model scores jobs, the relevance threshold, and what the worker is doing. */
export function ScoringSettings() {
  const { data, error, reload } = useApi<ScoringStatus>("/api/scoring/status");
  const [saving, setSaving] = useState(false);

  // The threshold box is edited locally and saved with the button.
  const [threshold, setThreshold] = useState("");
  const [lastSaved, setLastSaved] = useState<number | null>(null);
  if (data && data.threshold !== lastSaved) {
    setLastSaved(data.threshold);
    setThreshold(String(data.threshold));
  }

  // Keep the queue and worker status fresh.
  useEffect(() => {
    const timer = setInterval(reload, POLL_MS);
    return () => clearInterval(timer);
  }, [reload]);

  async function save(patch: SettingsPatch, message: string) {
    setSaving(true);
    try {
      await updateSettings(patch);
      toast.success(message);
      reload();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setSaving(false);
    }
  }

  function saveThreshold(event: FormEvent) {
    event.preventDefault();
    const value = Number(threshold);
    if (!Number.isInteger(value) || value < 0 || value > 100) {
      toast.error("Threshold must be a whole number from 0 to 100.");
      return;
    }
    void save({ relevance_threshold: value }, `Relevant now means ${value}+ with no blockers`);
  }

  // Offer every installed model, plus the selected one even if it isn't installed.
  const models = data
    ? Array.from(new Set([data.model, ...data.ollama.models])).sort()
    : [];

  return (
    <Card>
      <CardHeader>
        <CardTitle>Relevance scoring</CardTitle>
        <CardDescription>
          A local model reads each job next to your resume and scores the fit from 0 to 100. Compare
          models on the <Link href="/evals" className="text-primary underline-offset-4 hover:underline">Evals</Link> page.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        {error ? (
          <p className="text-sm text-danger">{error}</p>
        ) : !data ? (
          <Skeleton className="h-40 w-full" />
        ) : (
          <>
            <WorkerStatus status={data} />

            <div className="flex items-center justify-between gap-4">
              <div>
                <p className="text-sm font-medium">Score new jobs automatically</p>
                <p className="text-sm text-muted-foreground">
                  Off: only the Re-score button and eval runs use the model.
                </p>
              </div>
              <Switch
                label="Score new jobs automatically"
                checked={data.enabled}
                disabled={saving}
                onCheckedChange={(on) =>
                  save({ scoring_enabled: on }, on ? "Automatic scoring on" : "Automatic scoring off")
                }
              />
            </div>

            <div className="grid gap-4 sm:grid-cols-2">
              <label className="flex flex-col gap-1.5 text-sm">
                <span className="font-medium">Model</span>
                <Select
                  value={data.model}
                  disabled={saving}
                  onChange={(e) =>
                    save({ scoring_model: e.target.value }, `Scoring with ${e.target.value}`)
                  }
                >
                  {models.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </Select>
                <span className="text-xs text-muted-foreground">
                  Prompt version {data.prompt_version}. Changing model scores every recent job again.
                </span>
              </label>

              <form onSubmit={saveThreshold} className="flex flex-col gap-1.5 text-sm">
                <label htmlFor="threshold" className="font-medium">
                  Relevant from score
                </label>
                <div className="flex gap-2">
                  <Input
                    id="threshold"
                    type="number"
                    min={0}
                    max={100}
                    value={threshold}
                    onChange={(e) => setThreshold(e.target.value)}
                    className="w-24"
                  />
                  <Button
                    type="submit"
                    variant="outline"
                    disabled={saving || threshold === String(data.threshold)}
                  >
                    Save
                  </Button>
                </div>
                <span className="text-xs text-muted-foreground">
                  Jobs with a hard blocker are never relevant.
                </span>
              </form>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}

function WorkerStatus({ status }: { status: ScoringStatus }) {
  const { ollama, queue, worker } = status;
  const problem = !ollama.reachable
    ? (ollama.error ?? "Ollama is not reachable.")
    : !ollama.model_installed
      ? `${status.model} is not installed. Run: ollama pull ${status.model}`
      : worker.paused_reason;

  return (
    <div className="flex flex-col gap-3 rounded-lg bg-muted p-3 text-sm">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
        <span className="flex items-center gap-2">
          <span
            className={cn("size-2 rounded-full", ollama.reachable ? "bg-success" : "bg-danger")}
            aria-hidden
          />
          Ollama {ollama.reachable ? "online" : "offline"}
          <span className="text-muted-foreground">{ollama.url}</span>
        </span>
        <span className="text-muted-foreground tabular-nums">
          {queue.unscored} waiting · {queue.requested} requested
          {queue.failed_recently > 0 ? ` · ${queue.failed_recently} failed (retry in 1 h)` : ""}
        </span>
      </div>

      {problem ? (
        <p className="flex gap-2 text-warning">
          <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
          <span className="font-mono text-xs leading-5">{problem}</span>
        </p>
      ) : worker.current ? (
        <p className="flex items-center gap-2">
          <Loader2 className="size-4 animate-spin text-primary" aria-hidden />
          <span className="truncate">
            Scoring “{worker.current.title}” with {worker.current.model} (started{" "}
            {timeAgo(worker.current.started_at)})
          </span>
        </p>
      ) : (
        <p className="flex items-center gap-2 text-muted-foreground">
          <Cpu className="size-4" aria-hidden />
          {status.enabled ? "Idle. New jobs are scored within 30 seconds." : "Automatic scoring is off."}
          {worker.last_scored_at ? ` Last score ${timeAgo(worker.last_scored_at)}.` : ""}
        </p>
      )}
    </div>
  );
}
