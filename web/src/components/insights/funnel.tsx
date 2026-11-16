import type { Insights } from "@/lib/api";
import { funnelSteps, rate } from "@/lib/insights";

/** Applied → Replied → Interview → Offer, each bar as a share of Applied. */
export function Funnel({ funnel }: { funnel: Insights["funnel"] }) {
  return (
    <div className="flex flex-col gap-3">
      <ol className="flex flex-col gap-2.5">
        {funnelSteps(funnel).map((step) => (
          <li key={step.label} className="grid grid-cols-[5.5rem_1fr_4.5rem] items-center gap-3 text-sm">
            <span className="text-muted-foreground">{step.label}</span>
            <div className="h-5 overflow-hidden rounded bg-muted">
              <div
                className="h-full rounded bg-primary transition-all"
                style={{ width: `${Math.max(step.share * 100, step.count > 0 ? 2 : 0)}%` }}
              />
            </div>
            <span className="text-right font-medium tabular-nums">
              {step.count}
              {step.label !== "Applied" ? (
                <span className="ml-1 text-xs font-normal text-muted-foreground">{rate(step.share)}</span>
              ) : null}
            </span>
          </li>
        ))}
      </ol>
      <p className="text-xs text-muted-foreground tabular-nums">
        {funnel.rejected} rejected. A reply is the first move from Submitted to Interview, Offer or Rejected.
      </p>
    </div>
  );
}
