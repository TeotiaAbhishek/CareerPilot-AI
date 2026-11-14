import type { Metadata } from "next";
import { Suspense } from "react";

import { RunView } from "@/components/applications/run-view";
import { Skeleton } from "@/components/ui/skeleton";

export const metadata: Metadata = { title: "Apply run" };

/** One apply run: the live log, questions for you, and Submit. */
export default function RunPage({ params }: PageProps<"/applications/[id]">) {
  return (
    <Suspense fallback={<Skeleton className="h-96 w-full" />}>
      <RunFromParams params={params} />
    </Suspense>
  );
}

async function RunFromParams({ params }: { params: PageProps<"/applications/[id]">["params"] }) {
  const { id } = await params;
  return <RunView id={id} />;
}
