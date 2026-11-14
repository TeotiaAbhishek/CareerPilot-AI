import { Badge } from "@/components/ui/badge";
import { scoreTone } from "@/lib/scoring";
import { cn } from "@/lib/utils";

/** The score as a coloured pill. "–" until the job is scored. */
export function ScoreBadge({
  score,
  blockers,
  relevant,
  className,
}: {
  score: number | null;
  blockers: number | null;
  relevant: boolean | null;
  className?: string;
}) {
  const title =
    score === null
      ? "Not scored yet"
      : relevant
        ? "Relevant"
        : (blockers ?? 0) > 0
          ? "Has a hard blocker"
          : "Below your threshold";

  return (
    <Badge
      tone={scoreTone(score, blockers, relevant)}
      title={title}
      aria-label={score === null ? "Not scored yet" : `Score ${score}. ${title}`}
      className={cn("tabular-nums", className)}
    >
      {score ?? "–"}
    </Badge>
  );
}
