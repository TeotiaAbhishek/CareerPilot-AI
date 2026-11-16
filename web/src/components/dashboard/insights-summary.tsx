"use client";

import { ArrowRight, BarChart3 } from "lucide-react";
import Link from "next/link";

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import type { Insights } from "@/lib/api";
import { dayLabel } from "@/lib/insights";

/** A small version of Insights. The whole card opens the Insights page. */
export function InsightsSummary() {
  const { data, error } = useApi<Insights>("/api/insights");
  const weeks = data?.per_week ?? [];
  const thisWeek = weeks.at(-1)?.count ?? 0;
  const max = Math.max(1, ...weeks.map((w) => w.count));

  return (
    <Link
      href="/insights"
      className="group block rounded-xl focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
    >
      <Card className="h-full transition-colors group-hover:border-primary/40">
        <CardHeader>
          <div className="flex items-start justify-between gap-2">
            <div>
              <CardTitle className="flex items-center gap-2">
                <BarChart3 className="size-4 text-primary" aria-hidden /> Insights
              </CardTitle>
              <CardDescription>Applications per week, and what gets replies.</CardDescription>
            </div>
            <ArrowRight
              className="size-4 text-muted-foreground transition-transform group-hover:translate-x-0.5 group-hover:text-primary"
              aria-hidden
            />
          </div>
        </CardHeader>
        <CardContent>
          {error ? (
            <p className="text-sm text-danger">{error}</p>
          ) : !data ? (
            <Skeleton className="h-28 w-full" />
          ) : (
            <div className="flex flex-col gap-4">
              <dl className="grid grid-cols-3 gap-2 text-center">
                {[
                  { label: "This week", value: String(thisWeek) },
                  {
                    label: "Response rate",
                    value: data.response_rate === null ? "–" : `${Math.round(data.response_rate * 100)}%`,
                  },
                  { label: "Interviews", value: String(data.funnel.interview) },
                ].map((s) => (
                  <div key={s.label} className="rounded-lg bg-muted px-2 py-2">
                    <dd className="text-xl font-semibold tabular-nums">{s.value}</dd>
                    <dt className="text-xs text-muted-foreground">{s.label}</dt>
                  </div>
                ))}
              </dl>
              <div className="flex h-16 items-end gap-1" aria-label="Applications per week, last 12 weeks" role="img">
                {weeks.map((w) => (
                  <div
                    key={w.day}
                    title={`Week of ${dayLabel(w.day)}: ${w.count}`}
                    className="flex-1 rounded-t-sm bg-primary/70"
                    style={{ height: `${Math.max(4, (w.count / max) * 100)}%`, opacity: w.count ? 1 : 0.25 }}
                  />
                ))}
              </div>
            </div>
          )}
        </CardContent>
      </Card>
    </Link>
  );
}
