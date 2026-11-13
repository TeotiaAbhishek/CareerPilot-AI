import type { ComponentProps } from "react";

import { cn } from "@/lib/utils";

/** Grey pulsing block shown while data loads, shaped like the real content. */
export function Skeleton({ className, ...props }: ComponentProps<"div">) {
  return <div className={cn("animate-pulse rounded-md bg-muted", className)} {...props} />;
}
