import { Briefcase } from "lucide-react";
import type { Metadata } from "next";

import { EmptyState } from "@/components/ui/empty-state";
import { PageHeader } from "@/components/ui/page-header";

export const metadata: Metadata = { title: "Jobs" };

export default function Page() {
  return (
    <>
      <PageHeader title="Jobs" description="Every SEEK job CareerPilot has found, scored for fit." />
      <EmptyState icon={Briefcase} title="No jobs yet" description="The SEEK scraper arrives in the next feature. Jobs will appear here automatically." />
    </>
  );
}
