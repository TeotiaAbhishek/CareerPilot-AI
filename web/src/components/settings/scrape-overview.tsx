"use client";

import { CalendarClock } from "lucide-react";

import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import type { ScrapeRun, ScrapeStatus } from "@/lib/api";
import { formatDateTime, timeUntil } from "@/lib/format";

const STATUS_TONE = {
  running: "primary",
  succeeded: "success",
  failed: "danger",
  rate_limited: "warning",
} as const;

const STATUS_LABEL: Record<ScrapeRun["status"], string> = {
  running: "Running",
  succeeded: "Done",
  failed: "Failed",
  rate_limited: "Rate limited",
};

/** When the next scrape happens, and how the last few went. */
export function ScrapeOverview() {
  const { data, error } = useApi<ScrapeStatus>("/api/scrape/status");

  return (
    <Card>
      <CardHeader>
        <CardTitle>Schedule and history</CardTitle>
        <CardDescription>
          Change the time with <code className="font-mono">SCRAPE_DAILY_AT</code> in{" "}
          <code className="font-mono">backend/.env</code>.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {error ? (
          <p className="text-sm text-danger">{error}</p>
        ) : !data ? (
          <Skeleton className="h-32 w-full" />
        ) : (
          <>
            <div className="flex items-center gap-3 rounded-lg bg-muted p-3 text-sm">
              <CalendarClock className="size-4 text-muted-foreground" aria-hidden />
              {data.schedule.daily_at ? (
                <span>
                  Daily at <strong>{data.schedule.daily_at}</strong> ({data.schedule.timezone}) · next{" "}
                  {timeUntil(data.schedule.next_run_at)}
                </span>
              ) : (
                <span>Daily scrape is off. Use “Scrape now” on the Jobs page.</span>
              )}
            </div>

            {data.recent.length === 0 ? (
              <p className="text-sm text-muted-foreground">No scrapes yet.</p>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full text-left text-sm">
                  <thead className="text-xs text-muted-foreground">
                    <tr>
                      <th className="py-2 pr-4 font-medium">Started</th>
                      <th className="py-2 pr-4 font-medium">Trigger</th>
                      <th className="py-2 pr-4 font-medium">Status</th>
                      <th className="py-2 pr-4 text-right font-medium">Found</th>
                      <th className="py-2 pr-4 text-right font-medium">New</th>
                      <th className="py-2 font-medium">Message</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y">
                    {data.recent.map((run) => (
                      <tr key={run.id}>
                        <td className="whitespace-nowrap py-2 pr-4">{formatDateTime(run.started_at)}</td>
                        <td className="py-2 pr-4 capitalize">{run.trigger}</td>
                        <td className="py-2 pr-4">
                          <Badge tone={STATUS_TONE[run.status]}>{STATUS_LABEL[run.status]}</Badge>
                        </td>
                        <td className="py-2 pr-4 text-right tabular-nums">{run.jobs_found}</td>
                        <td className="py-2 pr-4 text-right tabular-nums">{run.jobs_new}</td>
                        <td className="max-w-xs truncate py-2 text-muted-foreground" title={run.message ?? ""}>
                          {run.message}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </>
        )}
      </CardContent>
    </Card>
  );
}
