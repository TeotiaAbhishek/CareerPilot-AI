import { FollowUpsDue } from "@/components/dashboard/follow-ups-due";
import { Roadmap } from "@/components/dashboard/roadmap";
import { StatsRow } from "@/components/dashboard/stats-row";
import { SystemStatus } from "@/components/dashboard/system-status";
import { PageHeader } from "@/components/ui/page-header";

export default function DashboardPage() {
  return (
    <>
      <PageHeader title="Dashboard" description="Your job search at a glance." />

      <StatsRow />

      <section className="mt-6">
        <FollowUpsDue />
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
