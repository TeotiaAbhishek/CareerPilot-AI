import { UserRound } from "lucide-react";
import type { Metadata } from "next";

import { EmptyState } from "@/components/ui/empty-state";
import { PageHeader } from "@/components/ui/page-header";

export const metadata: Metadata = { title: "Profile" };

export default function Page() {
  return (
    <>
      <PageHeader title="Profile" description="Your master resume, application facts and saved logins." />
      <EmptyState icon={UserRound} title="Profile not set up" description="Your resume, answers to common questions and the encrypted password vault will live here." />
    </>
  );
}
