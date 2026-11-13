import type { LucideIcon } from "lucide-react";

import { Card, CardContent } from "@/components/ui/card";

/** A number with a label. `value` is null until the feature that feeds it exists. */
export function StatCard({
  label,
  value,
  hint,
  icon: Icon,
}: {
  label: string;
  value: number | null;
  hint: string;
  icon: LucideIcon;
}) {
  return (
    <Card>
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
}
