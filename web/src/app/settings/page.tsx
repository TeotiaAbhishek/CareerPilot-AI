import { Settings } from "lucide-react";
import type { Metadata } from "next";

import { EmptyState } from "@/components/ui/empty-state";
import { PageHeader } from "@/components/ui/page-header";

export const metadata: Metadata = { title: "Settings" };

export default function Page() {
  return (
    <>
      <PageHeader title="Settings" description="Search terms, models and schedules." />
      <EmptyState icon={Settings} title="Nothing to configure yet" description="Scraper searches, Ollama model choice and schedules will be configurable here." />
    </>
  );
}
