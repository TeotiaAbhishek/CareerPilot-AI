import type { Metadata } from "next";

import { DraftingSettings } from "@/components/settings/drafting-settings";
import { ScoringSettings } from "@/components/settings/scoring-settings";
import { ScrapeOverview } from "@/components/settings/scrape-overview";
import { SearchesManager } from "@/components/settings/searches-manager";
import { PageHeader } from "@/components/ui/page-header";

export const metadata: Metadata = { title: "Settings" };

export default function SettingsPage() {
  return (
    <>
      <PageHeader title="Settings" description="What to search for, when, how jobs are scored and how documents are drafted." />
      <div className="flex flex-col gap-6">
        <SearchesManager />
        <ScrapeOverview />
        <ScoringSettings />
        <DraftingSettings />
      </div>
    </>
  );
}
