import type { Metadata } from "next";

import { InsightsView } from "@/components/insights/insights-view";
import { PageHeader } from "@/components/ui/page-header";

export const metadata: Metadata = { title: "Insights" };

export default function Page() {
  return (
    <>
      <PageHeader title="Insights" description="How your applications are going, and what gets replies." />
      <InsightsView />
    </>
  );
}
