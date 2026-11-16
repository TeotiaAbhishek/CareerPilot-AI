import type { Metadata } from "next";

import { TrackerView } from "@/components/tracker/tracker-view";
import { PageHeader } from "@/components/ui/page-header";

export const metadata: Metadata = { title: "Applications" };

export default function Page() {
  return (
    <>
      <PageHeader
        title="Applications"
        description="Every job you're applying to, from Ready to Offer. Drag a card, or use its … menu."
      />
      <TrackerView />
    </>
  );
}
