"use client";

import type { JobLabel } from "@/lib/api";
import { LABELS } from "@/lib/scoring";
import { cn } from "@/lib/utils";

const ACTIVE: Record<JobLabel, string> = {
  apply: "bg-success text-white",
  maybe: "bg-warning text-white",
  no: "bg-danger text-white",
};

/**
 * Your verdict on a job: Apply / Maybe / No. Click the selected one again to clear it.
 * Shortcuts on the Jobs page: a, m, n.
 */
export function LabelControl({
  value,
  onChange,
  disabled,
}: {
  value: JobLabel | null;
  onChange: (label: JobLabel | null) => void;
  disabled?: boolean;
}) {
  return (
    <div role="radiogroup" aria-label="Your label" className="inline-flex rounded-md border bg-muted p-0.5">
      {LABELS.map((option) => {
        const checked = value === option.value;
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={checked}
            disabled={disabled}
            onClick={() => onChange(checked ? null : option.value)}
            className={cn(
              "rounded px-3 py-1 text-sm font-medium transition-colors disabled:opacity-50",
              checked ? ACTIVE[option.value] : "text-muted-foreground hover:text-foreground",
            )}
          >
            {option.text}
          </button>
        );
      })}
    </div>
  );
}
