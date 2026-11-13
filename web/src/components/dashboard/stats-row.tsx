"use client";

import { Bookmark, Briefcase, Inbox, Sparkles } from "lucide-react";

import { useApi } from "@/hooks/use-api";
import type { Stats } from "@/lib/api";

import { StatCard } from "./stat-card";

/** The four numbers at the top of the dashboard. */
export function StatsRow() {
  const { data } = useApi<Stats>("/api/stats");

  return (
    <section aria-label="Stats" className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
      <StatCard label="New jobs" value={data?.jobs_new_24h ?? null} hint="Found in the last 24 h" icon={Briefcase} />
      <StatCard label="Unread" value={data?.jobs_unviewed ?? null} hint="Not opened yet" icon={Inbox} />
      <StatCard label="Saved" value={data?.jobs_saved ?? null} hint="Bookmarked to look at" icon={Bookmark} />
      <StatCard label="Relevant" value={null} hint="Scored 60+ (arrives in F3)" icon={Sparkles} />
    </section>
  );
}
