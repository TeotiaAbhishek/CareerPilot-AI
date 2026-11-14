import type { Metadata } from "next";

import { ApplicationsList } from "@/components/applications/applications-list";
import { PageHeader } from "@/components/ui/page-header";

export const metadata: Metadata = { title: "Applications" };

export default function Page() {
  return (
    <>
      <PageHeader title="Applications" description="Every apply run, with the ones that need you first." />
      <ApplicationsList />
    </>
  );
}
