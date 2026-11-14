"use client";

import { Bot, CircleCheck, FlaskConical, Loader2, LogIn, TriangleAlert } from "lucide-react";
import { useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Select } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { useApi } from "@/hooks/use-api";
import {
  type ApplyStatus,
  type ScoringStatus,
  type Settings,
  type SettingsPatch,
  errorMessage,
  openInAgentBrowser,
  startPractice,
  updateSettings,
} from "@/lib/api";

const POLL_MS = 5000;
const CAPS = [3, 5, 10, 15, 20, 30, 50];

/** The browser worker, how many applications run at once, and the daily limit. */
export function ApplySettings() {
  const router = useRouter();
  const status = useApi<ApplyStatus>("/api/apply/status");
  const settings = useApi<Settings>("/api/settings");
  const { data: scoring } = useApi<ScoringStatus>("/api/scoring/status");
  const [busy, setBusy] = useState(false);
  const reloadStatus = status.reload;

  useEffect(() => {
    const timer = setInterval(reloadStatus, POLL_MS);
    return () => clearInterval(timer);
  }, [reloadStatus]);

  async function save(patch: SettingsPatch, message: string) {
    setBusy(true);
    try {
      const fresh = await updateSettings(patch);
      settings.mutate(() => fresh);
      reloadStatus();
      toast.success(message);
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function run(action: () => Promise<void>) {
    setBusy(true);
    try {
      await action();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const s = status.data;
  const cfg = settings.data;
  const models = cfg ? Array.from(new Set([cfg.apply_model, ...(scoring?.ollama.models ?? [])])).sort() : [];

  return (
    <Card>
      <CardHeader>
        <CardTitle>Browser apply</CardTitle>
        <CardDescription>
          The agent fills in application forms in its own Chromium window and stops before Submit. It never clicks
          Submit and never solves a CAPTCHA.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        {status.error || settings.error ? (
          <p className="text-sm text-danger">{status.error ?? settings.error}</p>
        ) : !s || !cfg ? (
          <Skeleton className="h-40 w-full" />
        ) : (
          <>
            <div className="flex flex-col gap-2 rounded-lg bg-muted p-3 text-sm">
              {s.worker_ok ? (
                <p className="flex items-center gap-2">
                  <CircleCheck className="size-4 text-success" aria-hidden /> Browser worker running at{" "}
                  <span className="font-mono text-xs">{s.worker_url}</span>
                </p>
              ) : (
                <div className="flex gap-2 text-warning">
                  <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
                  <div>
                    <p>The browser worker isn&apos;t running. Start it in a terminal:</p>
                    <pre className="mt-1 font-mono text-xs text-foreground">
                      cd ~/code/careerpilot-ai/browser-worker && npm start
                    </pre>
                  </div>
                </div>
              )}
              {s.paused_reason && s.worker_ok ? (
                <p className="flex gap-2 text-warning">
                  <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden /> {s.paused_reason}
                </p>
              ) : null}
              <p className="flex items-center gap-2 text-muted-foreground tabular-nums">
                {s.running > 0 ? <Loader2 className="size-4 animate-spin text-primary" aria-hidden /> : <Bot className="size-4" aria-hidden />}
                {s.running} filling in · {s.queued} queued · {s.waiting_for_you} with questions · {s.ready_to_submit} ready
                to submit · {s.started_today} of {s.daily_cap} started in the last 24 hours
              </p>
            </div>

            <div className="grid gap-4 sm:grid-cols-2">
              <label className="flex flex-col gap-1.5 text-sm">
                <span className="font-medium">Applications at once</span>
                <Select
                  value={cfg.apply_parallel}
                  disabled={busy}
                  onChange={(e) => save({ apply_parallel: Number(e.target.value) }, `Up to ${e.target.value} at once`)}
                >
                  {[1, 2, 3, 4, 5].map((n) => (
                    <option key={n} value={n}>
                      {n}
                    </option>
                  ))}
                </Select>
                <span className="text-xs text-muted-foreground">One browser tab each. More at once looks less human.</span>
              </label>
              <label className="flex flex-col gap-1.5 text-sm">
                <span className="font-medium">Daily limit</span>
                <Select
                  value={cfg.apply_daily_cap}
                  disabled={busy}
                  onChange={(e) => save({ apply_daily_cap: Number(e.target.value) }, `At most ${e.target.value} a day`)}
                >
                  {Array.from(new Set([...CAPS, cfg.apply_daily_cap]))
                    .sort((a, b) => a - b)
                    .map((n) => (
                      <option key={n} value={n}>
                        {n} in 24 hours
                      </option>
                    ))}
                </Select>
                <span className="text-xs text-muted-foreground">New runs wait once the limit is reached.</span>
              </label>
              <label className="flex flex-col gap-1.5 text-sm">
                <span className="font-medium">Model for form questions</span>
                <Select
                  value={cfg.apply_model}
                  disabled={busy}
                  onChange={(e) => save({ apply_model: e.target.value }, `Answering with ${e.target.value}`)}
                >
                  {models.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </Select>
                <span className="text-xs text-muted-foreground">Only asked what your profile and answer bank don&apos;t cover.</span>
              </label>
              <div className="flex items-start justify-between gap-4">
                <div>
                  <p className="text-sm font-medium">Think before answering</p>
                  <p className="text-sm text-muted-foreground">Slower. Usually not needed for short form questions.</p>
                </div>
                <Switch
                  label="Think before answering"
                  checked={cfg.apply_think}
                  disabled={busy}
                  onCheckedChange={(on) => save({ apply_think: on }, on ? "Thinking on" : "Thinking off")}
                />
              </div>
            </div>

            <div className="flex flex-wrap gap-2">
              <Button
                variant="outline"
                disabled={busy || !s.worker_ok}
                onClick={() =>
                  run(async () => {
                    await openInAgentBrowser("https://www.seek.com.au/");
                    toast.success("Opened SEEK in the agent's browser. Sign in there once.");
                  })
                }
              >
                <LogIn /> Sign in to SEEK in the agent&apos;s browser
              </Button>
              <Button
                variant="outline"
                disabled={busy || !s.worker_ok}
                onClick={() =>
                  run(async () => {
                    const practice = await startPractice();
                    router.push(`/applications/${practice.id}`);
                  })
                }
              >
                <FlaskConical /> Practice run
              </Button>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}
