import type { Metadata } from "next";
import { Suspense } from "react";

import { ApplySettings } from "@/components/settings/apply-settings";
import { DraftingSettings } from "@/components/settings/drafting-settings";
import { EmailSettings } from "@/components/settings/email-settings";
import { FollowUpSettings } from "@/components/settings/follow-up-settings";
import { ScoringSettings } from "@/components/settings/scoring-settings";
import { ScrapeOverview } from "@/components/settings/scrape-overview";
import { SearchesManager } from "@/components/settings/searches-manager";
import { SeekCleanup } from "@/components/settings/seek-cleanup";
import { PageHeader } from "@/components/ui/page-header";

export const metadata: Metadata = { title: "Settings" };

export default function SettingsPage() {
  return (
    <>
      <PageHeader title="Settings" description="What to search for, when, how jobs are scored, how documents and emails are drafted and how the agent applies." />
      <div className="flex flex-col gap-6">
        <SearchesManager />
        <ScrapeOverview />
        <ScoringSettings />
        <DraftingSettings />
        <FollowUpSettings />
        <ApplySettings />
        {/* Reads ?gmail= after Google sends you back, so it needs a Suspense boundary. */}
        <Suspense>
          <EmailSettings />
        </Suspense>
        <SeekCleanup />
      </div>
    </>
  );
}
