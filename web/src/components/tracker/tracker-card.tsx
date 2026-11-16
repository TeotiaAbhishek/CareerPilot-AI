"use client";

import { Loader2, Send, Zap } from "lucide-react";
import Link from "next/link";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { Card } from "@/lib/api";
import { timeAgo } from "@/lib/format";
import { cn } from "@/lib/utils";

import { type CardActions, CardMenu } from "./card-menu";

/** One job on the board: what it is, where it's at, and what's next. */
export function TrackerCard({
  card,
  actions,
  busy,
  dragging,
}: {
  card: Card;
  actions: CardActions;
  busy?: boolean;
  dragging?: boolean;
}) {
  const quick = card.apply_type?.toLowerCase().includes("quick");
  return (
    <div
      // Keys pressed on a link or button inside stay there, so Space on
      // "Apply" doesn't pick the card up.
      onKeyDown={(e) => e.target !== e.currentTarget && e.stopPropagation()}
      className={cn(
        "rounded-lg border bg-card p-3 text-sm shadow-xs transition-shadow",
        dragging && "shadow-lg ring-2 ring-primary",
      )}
    >
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <Link
            href={`/jobs/${encodeURIComponent(card.job_id)}`}
            className="line-clamp-2 font-medium hover:text-primary hover:underline"
          >
            {card.title}
          </Link>
          {card.company ? <p className="truncate text-muted-foreground">{card.company}</p> : null}
        </div>
        <CardMenu card={card} actions={actions} />
      </div>
      <p className="mt-2 flex items-center gap-1.5 text-xs text-muted-foreground">
        {card.column === "applying" ? <Loader2 className="size-3 animate-spin text-primary" aria-hidden /> : null}
        {card.application_id && card.column !== "ready" ? (
          <Link
            href={`/applications/${card.application_id}`}
            className="hover:text-primary hover:underline"
          >
            {card.detail}
          </Link>
        ) : (
          card.detail
        )}
      </p>
      <div className="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
        {card.score !== null ? (
          <Badge className="py-0 tabular-nums" title="Relevance score">
            {card.score}
          </Badge>
        ) : null}
        {quick ? (
          <span className="inline-flex items-center gap-1 text-primary">
            <Zap className="size-3" aria-hidden /> Quick
          </span>
        ) : null}
        <span className="ml-auto tabular-nums">{timeAgo(card.applied_at ?? card.updated_at)}</span>
      </div>
      {card.column === "ready" ? (
        <Button
          size="sm"
          className="mt-3 w-full"
          disabled={busy}
          onClick={() => actions.apply(card)}
        >
          {busy ? <Loader2 className="animate-spin" /> : <Send />} Apply
        </Button>
      ) : null}
    </div>
  );
}
