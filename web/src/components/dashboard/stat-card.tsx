import type { LucideIcon } from "lucide-react";
import Link from "next/link";

import { Card, CardContent } from "@/components/ui/card";

/**
 * A number with a label. `value` is null while loading.
 * With `href`, the whole card is a link to the matching list.
 */
export function StatCard({
  label,
  value,
  hint,
  icon: Icon,
  href,
}: {
  label: string;
  value: number | null;
  hint: string;
  icon: LucideIcon;
  href?: string;
}) {
  const card = (
    <Card className={href ? "h-full transition-colors group-hover:border-primary/40" : undefined}>
      <CardContent className="flex items-start justify-between pt-5">
        <div>
          <p className="text-sm text-muted-foreground">{label}</p>
          <p className="mt-2 text-3xl font-semibold tabular-nums tracking-tight">{value ?? "—"}</p>
          <p className="mt-1 text-xs text-muted-foreground">{hint}</p>
        </div>
        <div className="flex size-9 items-center justify-center rounded-lg bg-primary/10 text-primary">
          <Icon className="size-4.5" aria-hidden />
        </div>
      </CardContent>
    </Card>
  );

  if (!href) return card;
  return (
    <Link
      href={href}
      aria-label={`${label}: ${value ?? "loading"}. Open the list.`}
      className="group block rounded-xl focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
    >
      {card}
    </Link>
  );
}
