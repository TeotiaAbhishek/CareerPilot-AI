"use client";

import { ArrowRight, Bot, ExternalLink, MoreHorizontal, Send, Trash2 } from "lucide-react";
import Link from "next/link";
import { useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import type { Card, Stage } from "@/lib/api";
import { moveTargets } from "@/lib/tracker";

/** What you can do to a card. Everything you can drag, you can also do from here. */
export type CardActions = {
  apply: (card: Card) => void;
  move: (card: Card, stage: Stage) => void;
  remove: (card: Card) => void;
};

const ITEM = "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-muted [&_svg]:size-4";

/** The "…" menu on a card: works with a keyboard and on a phone, where dragging doesn't. */
export function CardMenu({ card, actions }: { card: Card; actions: CardActions }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (event: MouseEvent | KeyboardEvent) => {
      if (event instanceof KeyboardEvent ? event.key === "Escape" : !root.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", close);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", close);
    };
  }, [open]);

  const run = (action: () => void) => () => {
    setOpen(false);
    action();
  };

  return (
    <div ref={root} className="relative">
      <Button
        variant="ghost"
        size="icon"
        className="size-7"
        aria-label={`Actions for ${card.title}`}
        aria-expanded={open}
        aria-haspopup="menu"
        onClick={() => setOpen((o) => !o)}
        // Keep a click on the menu from starting a drag.
        onMouseDown={(e) => e.stopPropagation()}
      >
        <MoreHorizontal />
      </Button>
      {open ? (
        <div
          role="menu"
          className="absolute right-0 top-8 z-30 flex w-52 flex-col rounded-lg border bg-card p-1 shadow-lg"
          onMouseDown={(e) => e.stopPropagation()}
        >
          {card.column === "ready" ? (
            <button role="menuitem" className={ITEM} onClick={run(() => actions.apply(card))}>
              <Send /> Apply
            </button>
          ) : null}
          <Link role="menuitem" className={ITEM} href={`/jobs/${encodeURIComponent(card.job_id)}`}>
            <ExternalLink /> Open the job
          </Link>
          {card.application_id ? (
            <Link role="menuitem" className={ITEM} href={`/applications/${card.application_id}`}>
              <Bot /> Open the run
            </Link>
          ) : null}
          <div className="my-1 border-t" />
          {moveTargets(card).map((target) => (
            <button
              key={target.id}
              role="menuitem"
              className={ITEM}
              onClick={run(() => actions.move(card, target.stage!))}
            >
              <ArrowRight /> Move to {target.label}
            </button>
          ))}
          <div className="my-1 border-t" />
          <button role="menuitem" className={`${ITEM} text-danger`} onClick={run(() => actions.remove(card))}>
            <Trash2 /> Remove from board
          </button>
        </div>
      ) : null}
    </div>
  );
}
