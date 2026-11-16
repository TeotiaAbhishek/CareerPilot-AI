"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect } from "react";

import { useApi } from "@/hooks/use-api";
import type { Email } from "@/lib/api";
import { cn } from "@/lib/utils";

import { NAV_ITEMS, isActive } from "./nav-items";

/** The list of links. Client component because it reads the current URL. */
const DUE_POLL_MS = 60_000;

export function NavLinks({ onNavigate }: { onNavigate?: () => void }) {
  const pathname = usePathname();

  // Emails due, shown next to Applications. Checked once a minute.
  const due = useApi<Email[]>("/api/emails/due");
  const { reload } = due;
  useEffect(() => {
    const timer = setInterval(reload, DUE_POLL_MS);
    return () => clearInterval(timer);
  }, [reload]);
  const counts: Record<string, number> = { "/applications": due.data?.length ?? 0 };

  return (
    <nav aria-label="Main" className="flex flex-col gap-1">
      {NAV_ITEMS.map(({ href, label, icon: Icon }) => {
        const active = isActive(pathname, href);
        return (
          <Link
            key={href}
            href={href}
            onClick={onNavigate}
            aria-current={active ? "page" : undefined}
            className={cn(
              "flex items-center gap-3 rounded-md px-3 py-2 text-sm font-medium transition-colors",
              active
                ? "bg-primary/10 text-primary"
                : "text-muted-foreground hover:bg-muted hover:text-foreground",
            )}
          >
            <Icon className="size-4" aria-hidden />
            {label}
            {counts[href] ? (
              <span
                className="ml-auto rounded-full bg-warning/15 px-2 text-xs font-semibold text-warning tabular-nums"
                aria-label={`${counts[href]} emails due`}
              >
                {counts[href]}
              </span>
            ) : null}
          </Link>
        );
      })}
    </nav>
  );
}

/** Shown for a split second if the URL isn't known yet (see Suspense in the sidebar). */
export function NavLinksFallback() {
  return (
    <div className="flex flex-col gap-1">
      {NAV_ITEMS.map(({ href }) => (
        <div key={href} className="h-9 animate-pulse rounded-md bg-muted/60" />
      ))}
    </div>
  );
}
