"use client";

import { ArrowDown, ArrowUp, Check, CircleAlert, Trash2, Undo2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import type { DraftLine, Flag } from "@/lib/api";
import { flagsAt } from "@/lib/documents";
import { cn } from "@/lib/utils";

/** What every editable line needs from the review screen. */
export type LineContext = {
  flags: Flag[];
  dismissed: string[];
  evidence: Map<string, string>;
  onDismiss: (key: string, dismissed: boolean) => void;
  disabled?: boolean;
};

/**
 * One editable sentence, bullet or paragraph: a text box, the evidence it
 * cites (hover or focus a chip to read it), and the checker's flags.
 */
export function DraftLineEditor({
  label,
  line,
  path,
  ctx,
  onChange,
  onRemove,
  onMove,
}: {
  label: string;
  line: DraftLine;
  path: string;
  ctx: LineContext;
  onChange: (text: string) => void;
  onRemove?: () => void;
  onMove?: (direction: -1 | 1) => void;
}) {
  const { open, dismissed } = flagsAt(ctx.flags, path, ctx.dismissed);
  const hasErrors = open.some((f) => f.severity === "error");

  return (
    <div className="flex flex-col gap-1.5" data-path={path}>
      <div className="flex gap-2">
        <Textarea
          aria-label={label}
          aria-invalid={hasErrors || undefined}
          value={line.text}
          disabled={ctx.disabled}
          rows={2}
          onChange={(e) => onChange(e.target.value)}
          // field-sizing-content grows the box to fit its text.
          className={cn("field-sizing-content min-h-16 resize-y leading-relaxed", hasErrors && "border-danger/60 bg-danger/5")}
        />
        {onRemove || onMove ? (
          <div className="flex shrink-0 flex-col gap-1">
            {onMove ? (
              <>
                <Button variant="ghost" size="icon" className="size-7" aria-label={`Move ${label} up`} onClick={() => onMove(-1)} disabled={ctx.disabled}>
                  <ArrowUp />
                </Button>
                <Button variant="ghost" size="icon" className="size-7" aria-label={`Move ${label} down`} onClick={() => onMove(1)} disabled={ctx.disabled}>
                  <ArrowDown />
                </Button>
              </>
            ) : null}
            {onRemove ? (
              <Button variant="ghost" size="icon" className="size-7 text-muted-foreground hover:text-danger" aria-label={`Remove ${label}`} onClick={onRemove} disabled={ctx.disabled}>
                <Trash2 />
              </Button>
            ) : null}
          </div>
        ) : null}
      </div>

      <div className="flex flex-wrap items-center gap-1.5">
        {line.evidence.length === 0 ? (
          <span className="text-xs text-muted-foreground">No evidence cited</span>
        ) : (
          line.evidence.map((id) => <EvidenceChip key={id} id={id} text={ctx.evidence.get(id)} />)
        )}
      </div>

      {open.map((flag) => (
        <div
          key={flag.key}
          className={cn(
            "flex flex-wrap items-center gap-2 rounded-md px-2.5 py-1.5 text-xs",
            flag.severity === "error" ? "bg-danger/10 text-danger" : "bg-muted text-muted-foreground",
          )}
        >
          <CircleAlert className="size-3.5 shrink-0" aria-hidden />
          <span className="min-w-0 flex-1">{flag.message}</span>
          {flag.severity === "error" ? (
            <button
              type="button"
              disabled={ctx.disabled}
              onClick={() => ctx.onDismiss(flag.key, true)}
              className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 font-medium hover:bg-danger/15"
            >
              <Check className="size-3" aria-hidden /> It&apos;s true, keep it
            </button>
          ) : null}
        </div>
      ))}
      {dismissed.map((flag) => (
        <div key={flag.key} className="flex flex-wrap items-center gap-2 px-2.5 text-xs text-muted-foreground">
          <Check className="size-3.5 shrink-0 text-success" aria-hidden />
          <span className="min-w-0 flex-1 line-through decoration-muted-foreground/40">{flag.message}</span>
          <button
            type="button"
            disabled={ctx.disabled}
            onClick={() => ctx.onDismiss(flag.key, false)}
            className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 hover:bg-muted"
          >
            <Undo2 className="size-3" aria-hidden /> Undo
          </button>
        </div>
      ))}
    </div>
  );
}

/** An evidence id. Hover or focus it to see the line of your resume it points to. */
export function EvidenceChip({ id, text }: { id: string; text: string | undefined }) {
  return (
    <span className="group relative">
      <span
        tabIndex={0}
        aria-label={text ? `Evidence ${id}: ${text}` : `Evidence ${id} (not in your resume)`}
        className={cn(
          "inline-flex cursor-help items-center rounded border px-1.5 font-mono text-[11px] leading-5 outline-none focus-visible:ring-2 focus-visible:ring-ring",
          text ? "border-primary/30 bg-primary/10 text-primary" : "border-danger/30 bg-danger/10 text-danger",
        )}
      >
        {id}
      </span>
      <span
        role="tooltip"
        // display:none (not just invisible) so hidden tooltips never widen the page on phones.
        className="pointer-events-none absolute bottom-full left-0 z-20 mb-1.5 hidden w-72 max-w-[70vw] rounded-md border bg-card p-2.5 text-xs leading-relaxed text-card-foreground shadow-lg group-focus-within:block group-hover:block"
      >
        <span className="mb-1 block font-mono text-[11px] text-muted-foreground">{id}</span>
        {text ?? "This id isn't in your resume."}
      </span>
    </span>
  );
}
