import type { Metadata } from "next";

import { DocumentsList } from "@/components/documents/documents-list";
import { PageHeader } from "@/components/ui/page-header";

export const metadata: Metadata = { title: "Documents" };

export default function Page() {
  return (
    <>
      <PageHeader
        title="Documents"
        description="Tailored resumes and cover letters. Only approved ones are used when applying."
      />
      <DocumentsList />
    </>
  );
}
