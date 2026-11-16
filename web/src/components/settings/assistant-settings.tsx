"use client";

import { useState } from "react";
import { toast } from "sonner";

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Select } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { useApi } from "@/hooks/use-api";
import { type ScoringStatus, type Settings, type SettingsPatch, errorMessage, updateSettings } from "@/lib/api";

/** The model behind the Ask page. */
export function AssistantSettings() {
  const { data, error, mutate } = useApi<Settings>("/api/settings");
  // The installed models come from the scoring status (same Ollama).
  const { data: scoring } = useApi<ScoringStatus>("/api/scoring/status");
  const [busy, setBusy] = useState(false);

  async function save(patch: SettingsPatch, message: string) {
    setBusy(true);
    try {
      const fresh = await updateSettings(patch);
      mutate(() => fresh);
      toast.success(message);
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const models = data ? Array.from(new Set([data.assistant_model, ...(scoring?.ollama.models ?? [])])).sort() : [];

  return (
    <Card>
      <CardHeader>
        <CardTitle>Ask assistant</CardTitle>
        <CardDescription>
          The local model that answers on the Ask page. It needs a model that can call tools, such as qwen3. It only
          reads your data and never sees your contact details.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {error ? (
          <p className="text-sm text-danger">{error}</p>
        ) : !data ? (
          <Skeleton className="h-16 w-full" />
        ) : (
          <div className="grid gap-4 sm:grid-cols-2">
            <label className="flex flex-col gap-1.5 text-sm">
              <span className="font-medium">Model</span>
              <Select
                value={data.assistant_model}
                disabled={busy}
                onChange={(e) => save({ assistant_model: e.target.value }, `Answering with ${e.target.value}`)}
              >
                {models.map((m) => (
                  <option key={m} value={m}>
                    {m}
                  </option>
                ))}
              </Select>
              <span className="text-xs text-muted-foreground">qwen3:8b answers faster; qwen3:14b answers better.</span>
            </label>
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="text-sm font-medium">Think before answering</p>
                <p className="text-sm text-muted-foreground">Slower. Can help with harder questions.</p>
              </div>
              <Switch
                label="Think before answering"
                checked={data.assistant_think}
                disabled={busy}
                onCheckedChange={(on) => save({ assistant_think: on }, on ? "Thinking on" : "Thinking off")}
              />
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
