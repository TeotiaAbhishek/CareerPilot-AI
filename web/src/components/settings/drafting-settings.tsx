"use client";

import { Loader2, PenLine, TriangleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Select } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { useApi } from "@/hooks/use-api";
import { type DrafterStatus, type ScoringStatus, type SettingsPatch, errorMessage, updateSettings } from "@/lib/api";
import { KIND_LABEL } from "@/lib/documents";
import { timeAgo } from "@/lib/format";

const POLL_MS = 5000;

/** Which local model writes tailored documents, and whether it thinks first. */
export function DraftingSettings() {
  const { data, error, reload } = useApi<DrafterStatus>("/api/documents/status");
  // The installed models come from the scoring status (same Ollama).
  const { data: scoring } = useApi<ScoringStatus>("/api/scoring/status");
  const [saving, setSaving] = useState(false);

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

  const models = data ? Array.from(new Set([data.model, ...(scoring?.ollama.models ?? [])])).sort() : [];

  return (
    <Card>
      <CardHeader>
        <CardTitle>Tailored documents</CardTitle>
        <CardDescription>
          The local model that drafts resumes and cover letters when you click Prepare application.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        {error ? (
          <p className="text-sm text-danger">{error}</p>
        ) : !data ? (
          <Skeleton className="h-28 w-full" />
        ) : (
          <>
            <div className="flex flex-col gap-2 rounded-lg bg-muted p-3 text-sm">
              {data.worker.paused_reason ? (
                <p className="flex gap-2 text-warning">
                  <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
                  <span className="font-mono text-xs leading-5">{data.worker.paused_reason}</span>
                </p>
              ) : data.worker.current ? (
                <p className="flex items-center gap-2">
                  <Loader2 className="size-4 animate-spin text-primary" aria-hidden />
                  Writing a {KIND_LABEL[data.worker.current.kind].toLowerCase()} (started{" "}
                  {timeAgo(data.worker.current.started_at)})
                </p>
              ) : (
                <p className="flex items-center gap-2 text-muted-foreground">
                  <PenLine className="size-4" aria-hidden /> Idle.
                </p>
              )}
              <p className="text-muted-foreground tabular-nums">{data.queued} in the queue</p>
            </div>

            <div className="grid gap-4 sm:grid-cols-2">
              <label className="flex flex-col gap-1.5 text-sm">
                <span className="font-medium">Model</span>
                <Select
                  value={data.model}
                  disabled={saving}
                  onChange={(e) => save({ drafting_model: e.target.value }, `Drafting with ${e.target.value}`)}
                >
                  {models.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </Select>
                <span className="text-xs text-muted-foreground">Bigger models write better but slower.</span>
              </label>

              <div className="flex items-start justify-between gap-4">
                <div>
                  <p className="text-sm font-medium">Think before writing</p>
                  <p className="text-sm text-muted-foreground">
                    Better drafts, about twice as slow. Turn off if your model doesn&apos;t support thinking.
                  </p>
                </div>
                <Switch
                  label="Think before writing"
                  checked={data.think}
                  disabled={saving}
                  onCheckedChange={(on) => save({ drafting_think: on }, on ? "Thinking on" : "Thinking off")}
                />
              </div>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}
