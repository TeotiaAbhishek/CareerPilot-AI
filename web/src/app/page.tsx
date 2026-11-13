import { Briefcase, CalendarCheck, Send, Sparkles } from "lucide-react";

import { Roadmap } from "@/components/dashboard/roadmap";
import { StatCard } from "@/components/dashboard/stat-card";
import { SystemStatus } from "@/components/dashboard/system-status";
import { PageHeader } from "@/components/ui/page-header";

export default function DashboardPage() {
  return (
    <>
      <PageHeader title="Dashboard" description="Your job search at a glance." />

      <section aria-label="Stats" className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <StatCard label="New jobs" value={null} hint="Scraped in the last 24 h" icon={Briefcase} />
        <StatCard label="Relevant" value={null} hint="Scored 60 or more" icon={Sparkles} />
        <StatCard label="Applied" value={null} hint="This month" icon={Send} />
        <StatCard label="Interviews" value={null} hint="Upcoming" icon={CalendarCheck} />
      </section>

      <section className="mt-6 grid gap-4 lg:grid-cols-3">
        <div className="lg:col-span-2">
          <Roadmap />
        </div>
        <SystemStatus />
      </section>
    </>
  );
}
