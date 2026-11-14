import type { Metadata } from "next";
import { Suspense } from "react";

import { DocumentPreview } from "@/components/documents/document-preview";
import { Skeleton } from "@/components/ui/skeleton";

export const metadata: Metadata = { title: "Print" };

/** A print view of one document, for "Save as PDF". */
export default function PrintPage({ params }: PageProps<"/documents/[jobId]/print">) {
  return (
    <Suspense fallback={<Skeleton className="mx-auto h-[29.7cm] w-full max-w-[21cm]" />}>
      <PreviewFromParams params={params} />
    </Suspense>
  );
}

async function PreviewFromParams({ params }: { params: PageProps<"/documents/[jobId]/print">["params"] }) {
  const { jobId } = await params;
  return <DocumentPreview jobId={decodeURIComponent(jobId)} />;
}
