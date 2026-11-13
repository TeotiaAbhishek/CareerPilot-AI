"use client";

import { Loader2, RefreshCw } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { type ScrapeRun, errorMessage, getScrapeStatus, startScrape } from "@/lib/api";

const POLL_MS = 2_000;

/**
 * "Scrape now" plus live progress. While a run is going (started here, by the
 * schedule, or in another tab) it polls the status every 2 seconds.
 */
export function ScrapeButton({ onFinished }: { onFinished?: () => void }) {
  const [run, setRun] = useState<ScrapeRun | null>(null);
  const [starting, setStarting] = useState(false);
  // Bumped to restart polling after we start a run ourselves.
  const [pollKey, setPollKey] = useState(0);
  const onFinishedRef = useRef(onFinished);

  useEffect(() => {
    onFinishedRef.current = onFinished;
  }, [onFinished]);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let wasRunning = false;

    async function poll() {
      try {
        const status = await getScrapeStatus();
        if (cancelled) return;
        setRun(status.current);

        if (status.current) {
          wasRunning = true;
          timer = setTimeout(poll, POLL_MS);
        } else if (wasRunning) {
          // It just finished: tell the user how it went and refresh the list.
          const last = status.recent[0];
          if (last?.status === "succeeded") toast.success(last.message ?? "Scrape finished.");
          else if (last) toast.error(last.message ?? "Scrape failed.");
          onFinishedRef.current?.();
        }
      } catch {
        // API down: the dashboard's status card covers that. Try again later.
        if (!cancelled) timer = setTimeout(poll, POLL_MS * 5);
      }
    }

    void poll();
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [pollKey]);

  async function handleClick() {
    setStarting(true);
    try {
      setRun(await startScrape());
      setPollKey((k) => k + 1);
    } catch (error) {
      toast.error(errorMessage(error));
    } finally {
      setStarting(false);
    }
  }

  const busy = starting || run !== null;

  return (
    <div className="flex items-center gap-3">
      {run ? (
        <p className="hidden text-right text-xs text-muted-foreground sm:block" aria-live="polite">
          {run.message}
          <br />
          <span className="tabular-nums">
            {run.jobs_found} found · {run.jobs_new} new
          </span>
        </p>
      ) : null}
      <Button onClick={handleClick} disabled={busy}>
        {busy ? <Loader2 className="animate-spin" /> : <RefreshCw />}
        {busy ? "Scraping…" : "Scrape now"}
      </Button>
    </div>
  );
}
