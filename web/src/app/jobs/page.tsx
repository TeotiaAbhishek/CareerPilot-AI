import type { Metadata } from "next";
import { Suspense } from "react";

import { JobsView } from "@/components/jobs/jobs-view";
import { JobListSkeleton } from "@/components/jobs/job-list";

export const metadata: Metadata = { title: "Jobs" };

export default function JobsPage() {
  // JobsView reads the URL's ?query, which isn't known when the page is
  // pre-rendered, so it renders inside Suspense with a skeleton as fallback.
  return (
    <Suspense fallback={<JobListSkeleton />}>
      <JobsView />
    </Suspense>
  );
}
