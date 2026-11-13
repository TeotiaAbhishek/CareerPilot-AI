import { FileText } from "lucide-react";
import type { Metadata } from "next";

import { EmptyState } from "@/components/ui/empty-state";
import { PageHeader } from "@/components/ui/page-header";

export const metadata: Metadata = { title: "Documents" };

export default function Page() {
  return (
    <>
      <PageHeader title="Documents" description="Tailored resumes and cover letters for the jobs you apply to." />
      <EmptyState icon={FileText} title="No documents yet" description="Documents are generated when you click Apply on a job." />
    </>
  );
}
