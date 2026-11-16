"use client";

import { CalendarClock, MessageSquareReply, Send, Users } from "lucide-react";
import Link from "next/link";

import { StatCard } from "@/components/dashboard/stat-card";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import type { Insights } from "@/lib/api";
import { days, rate } from "@/lib/insights";

import { ActivityChart } from "./activity-chart";
import { BreakdownTable } from "./breakdown-table";
import { Funnel } from "./funnel";

/** How your applications are going: volume, replies, and what works best. */
export function InsightsView() {
  const { data, error } = useApi<Insights>("/api/insights");

  if (error) {
    return <div className="rounded-xl border border-danger/30 bg-danger/5 p-6 text-sm text-danger">{error}</div>;
  }
  if (!data) {
    return (
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {[0, 1, 2, 3].map((i) => (
          <Skeleton key={i} className="h-28" />
        ))}
        <Skeleton className="h-72 sm:col-span-2 lg:col-span-4" />
      </div>
    );
  }
  if (data.funnel.applied === 0) {
    return (
      <EmptyState
        icon={Send}
        title="No applications yet"
        description="Once you submit applications (or move cards to Submitted), your numbers show up here."
        action={
          <Link href="/applications" className="text-sm font-medium text-primary hover:underline">
            Open Applications
          </Link>
        }
      />
    );
  }

  return (
    <div className="flex flex-col gap-6">
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <StatCard label="Applied" value={data.funnel.applied} hint="Submitted or further along" icon={Send} />
        <StatCard
          label="Replies"
          value={data.funnel.replied}
          hint={`${rate(data.response_rate)} response rate`}
          icon={MessageSquareReply}
        />
        <StatCard
          label="Interviews"
          value={data.funnel.interview}
          hint={`${data.funnel.offer} offer${data.funnel.offer === 1 ? "" : "s"}`}
          icon={Users}
        />
        <Card>
          <CardContent className="flex items-start justify-between pt-5">
            <div>
              <p className="text-sm text-muted-foreground">Time to first reply</p>
              <p className="mt-2 text-3xl font-semibold tabular-nums tracking-tight">
                {days(data.median_days_to_reply)}
              </p>
              <p className="mt-1 text-xs text-muted-foreground">Median, from applying</p>
            </div>
            <div className="flex size-9 items-center justify-center rounded-lg bg-primary/10 text-primary">
              <CalendarClock className="size-4.5" aria-hidden />
            </div>
          </CardContent>
        </Card>
      </div>

      <div className="grid gap-6 lg:grid-cols-[minmax(0,3fr)_minmax(0,2fr)]">
        <Card>
          <CardHeader>
            <CardTitle>Applications</CardTitle>
          </CardHeader>
          <CardContent>
            <ActivityChart perDay={data.per_day} perWeek={data.per_week} />
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle>Funnel</CardTitle>
            <CardDescription>How far your applications get.</CardDescription>
          </CardHeader>
          <CardContent>
            <Funnel funnel={data.funnel} />
          </CardContent>
        </Card>
      </div>

      <div className="grid gap-6 lg:grid-cols-3">
        <Card>
          <CardHeader>
            <CardTitle>By relevance score</CardTitle>
            <CardDescription>Do better-scored jobs reply more?</CardDescription>
          </CardHeader>
          <CardContent>
            <BreakdownTable rows={data.by_score} group="Score" />
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle>By how you applied</CardTitle>
            <CardDescription>SEEK Quick Apply or the employer&apos;s own site.</CardDescription>
          </CardHeader>
          <CardContent>
            <BreakdownTable rows={data.by_apply_type} group="Apply type" />
          </CardContent>
        </Card>
        <Card>
          <CardHeader>
            <CardTitle>By source</CardTitle>
            <CardDescription>Where the job came from.</CardDescription>
          </CardHeader>
          <CardContent>
            <BreakdownTable rows={data.by_source} group="Source" />
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
