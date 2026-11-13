import { ArrowLeft } from "lucide-react";
import type { Metadata } from "next";
import Link from "next/link";
import { Suspense } from "react";

import { JobDetailSkeleton, JobDetailView } from "@/components/jobs/job-detail";

export const metadata: Metadata = { title: "Job" };

/** A job on its own page: used on phones, and for links to a single job. */
export default function JobPage({ params }: PageProps<"/jobs/[id]">) {
  return (
    <>
      <Link
        href="/jobs"
        className="mb-4 inline-flex items-center gap-1.5 text-sm text-muted-foreground hover:text-foreground"
      >
        <ArrowLeft className="size-4" aria-hidden /> All jobs
      </Link>
      {/* The id comes from the URL, so it's only known at request time. */}
      <Suspense fallback={<JobDetailSkeleton />}>
        <JobFromParams params={params} />
      </Suspense>
    </>
  );
}

async function JobFromParams({ params }: { params: PageProps<"/jobs/[id]">["params"] }) {
  const { id } = await params;
  return <JobDetailView id={decodeURIComponent(id)} />;
}
