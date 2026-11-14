import type { Metadata } from "next";
import { Suspense } from "react";

import { DocumentReview } from "@/components/documents/document-review";
import { Skeleton } from "@/components/ui/skeleton";

export const metadata: Metadata = { title: "Review documents" };

/** Review, edit and approve the tailored resume and cover letter for one job. */
export default function ReviewPage({ params }: PageProps<"/documents/[jobId]">) {
  return (
    // The job id comes from the URL and the tab from ?tab=, so both wait for the request.
    <Suspense fallback={<Skeleton className="h-96 w-full" />}>
      <ReviewFromParams params={params} />
    </Suspense>
  );
}

async function ReviewFromParams({ params }: { params: PageProps<"/documents/[jobId]">["params"] }) {
  const { jobId } = await params;
  return <DocumentReview jobId={decodeURIComponent(jobId)} />;
}
