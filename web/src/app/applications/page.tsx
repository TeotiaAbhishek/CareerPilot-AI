import { KanbanSquare } from "lucide-react";
import type { Metadata } from "next";

import { EmptyState } from "@/components/ui/empty-state";
import { PageHeader } from "@/components/ui/page-header";

export const metadata: Metadata = { title: "Applications" };

export default function Page() {
  return (
    <>
      <PageHeader title="Applications" description="Track every application from Apply to offer." />
      <EmptyState icon={KanbanSquare} title="No applications yet" description="Click Apply on a job and it will show up here with its status." />
    </>
  );
}
