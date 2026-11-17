import type { Metadata } from "next";
import { Suspense } from "react";

import { ApplySettings } from "@/components/settings/apply-settings";
import { AssistantSettings } from "@/components/settings/assistant-settings";
import { CloudSettings } from "@/components/settings/cloud-settings";
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
      <PageHeader title="Settings" description="What to search for, when, which models do the work (local, Claude or OpenAI), how the agent applies and which model answers on Ask." />
      <div className="flex flex-col gap-6">
        <SearchesManager />
        <ScrapeOverview />
        <CloudSettings />
        <ScoringSettings />
        <DraftingSettings />
        <FollowUpSettings />
        <ApplySettings />
        <AssistantSettings />
        {/* Reads ?gmail= after Google sends you back, so it needs a Suspense boundary. */}
        <Suspense>
          <EmailSettings />
        </Suspense>
        <SeekCleanup />
      </div>
    </>
  );
}
