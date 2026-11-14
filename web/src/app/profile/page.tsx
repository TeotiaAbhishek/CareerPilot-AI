import type { Metadata } from "next";
import { Suspense } from "react";

import { ProfileView } from "@/components/profile/profile-view";
import { Skeleton } from "@/components/ui/skeleton";

export const metadata: Metadata = { title: "Profile" };

export default function ProfilePage() {
  // The open tab lives in the URL (?tab=vault), which needs Suspense.
  return (
    <Suspense fallback={<Skeleton className="h-96 w-full" />}>
      <ProfileView />
    </Suspense>
  );
}
