import { Compass } from "lucide-react";
import Link from "next/link";

import { buttonVariants } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";

export default function NotFound() {
  return (
    <EmptyState
      icon={Compass}
      title="Page not found"
      description="That page doesn't exist. It may have moved, or the link is wrong."
      action={
        <Link href="/" className={buttonVariants()}>
          Back to dashboard
        </Link>
      }
    />
  );
}
