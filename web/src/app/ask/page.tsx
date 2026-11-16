import type { Metadata } from "next";
import { Suspense } from "react";

import { AskView } from "@/components/assistant/ask-view";
import { PageHeader } from "@/components/ui/page-header";
import { Skeleton } from "@/components/ui/skeleton";

export const metadata: Metadata = { title: "Ask" };

export default function Page() {
  return (
    <>
      <PageHeader
        title="Ask"
        description="Ask about your jobs, applications and resume. Answers come from your own data and local model."
      />
      {/* Reads ?chat= from the URL, so it needs a Suspense boundary. */}
      <Suspense fallback={<Skeleton className="h-[28rem] w-full" />}>
        <AskView />
      </Suspense>
    </>
  );
}
