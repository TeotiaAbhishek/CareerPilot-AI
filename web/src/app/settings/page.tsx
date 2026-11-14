import type { Metadata } from "next";

import { ScoringSettings } from "@/components/settings/scoring-settings";
import { ScrapeOverview } from "@/components/settings/scrape-overview";
import { SearchesManager } from "@/components/settings/searches-manager";
import { PageHeader } from "@/components/ui/page-header";

export const metadata: Metadata = { title: "Settings" };

export default function SettingsPage() {
  return (
    <>
      <PageHeader title="Settings" description="What to search for, when, and how jobs are scored." />
      <div className="flex flex-col gap-6">
        <SearchesManager />
        <ScrapeOverview />
        <ScoringSettings />
      </div>
    </>
  );
}
