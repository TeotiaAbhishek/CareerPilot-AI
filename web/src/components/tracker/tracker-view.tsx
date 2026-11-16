"use client";

import { Bot, KanbanSquare, List } from "lucide-react";
import Link from "next/link";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";

import { ApplicationsList } from "@/components/applications/applications-list";
import { EmptyState } from "@/components/ui/empty-state";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import { type Card, type Stage, errorMessage, moveCard, oneClickApply, removeCard } from "@/lib/api";
import { STARTED, columnInfo } from "@/lib/tracker";
import { cn } from "@/lib/utils";

import { Board } from "./board";
import { CardList } from "./card-list";
import type { CardActions } from "./card-menu";

const POLL_MS = 5000;
const VIEWS = [
  { id: "board", label: "Board", icon: KanbanSquare },
  { id: "list", label: "List", icon: List },
  { id: "runs", label: "Agent runs", icon: Bot },
] as const;
type View = (typeof VIEWS)[number]["id"];

/** The Applications page: your pipeline as a board or a list, plus every agent run. */
export function TrackerView() {
  const { data, error, loading, reload, mutate } = useApi<Card[]>("/api/tracker");
  const [view, setView] = useState<View>("board");
  const [busyId, setBusyId] = useState<string | null>(null);

  // While the agent works on something, check every few seconds.
  const working = data?.some((c) => c.column === "applying") ?? false;
  useEffect(() => {
    if (!working) return;
    const timer = setInterval(reload, POLL_MS);
    return () => clearInterval(timer);
  }, [working, reload]);

  const replace = useCallback(
    (card: Card) => mutate((cards) => cards.map((c) => (c.job_id === card.job_id ? card : c))),
    [mutate],
  );

  const actions: CardActions = {
    apply: async (card) => {
      setBusyId(card.job_id);
      try {
        const { started } = await oneClickApply(card.job_id);
        toast.success(STARTED[started]);
        reload();
      } catch (e) {
        toast.error(errorMessage(e));
      } finally {
        setBusyId(null);
      }
    },
    move: async (card: Card, stage: Stage) => {
      // Move it on screen straight away; put it back if the server says no.
      replace({ ...card, column: stage });
      try {
        replace(await moveCard(card.job_id, stage));
        toast.success(`Moved to ${columnInfo(stage).label}`);
      } catch (e) {
        replace(card);
        toast.error(errorMessage(e));
      }
    },
    remove: async (card) => {
      try {
        await removeCard(card.job_id);
        mutate((cards) => cards.filter((c) => c.job_id !== card.job_id));
        toast.success("Removed from the board. The job, its documents and runs stay.");
      } catch (e) {
        toast.error(errorMessage(e));
      }
    },
  };

  return (
    <div className="flex flex-col gap-4">
      <div role="tablist" aria-label="View" className="flex w-fit gap-1 rounded-lg border bg-card p-1">
        {VIEWS.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            role="tab"
            aria-selected={view === id}
            onClick={() => setView(id)}
            className={cn(
              "flex items-center gap-1.5 rounded-md px-3 py-1.5 text-sm font-medium transition-colors [&_svg]:size-4",
              view === id ? "bg-primary text-primary-foreground" : "text-muted-foreground hover:bg-muted",
            )}
          >
            <Icon aria-hidden /> {label}
          </button>
        ))}
      </div>

      {view === "runs" ? (
        <ApplicationsList />
      ) : error ? (
        <div className="rounded-xl border border-danger/30 bg-danger/5 p-6 text-sm text-danger">{error}</div>
      ) : loading && !data ? (
        <div className="flex gap-3 overflow-hidden">
          {[0, 1, 2, 3].map((i) => (
            <Skeleton key={i} className="h-64 w-64 shrink-0" />
          ))}
        </div>
      ) : !data?.length ? (
        <EmptyState
          icon={KanbanSquare}
          title="Nothing on your board yet"
          description="Click Apply on a job, or tick a few on the Jobs page and click Apply to selected. They show up here."
          action={
            <Link href="/jobs?sort=best_fit" className="text-sm font-medium text-primary hover:underline">
              Find a job to apply to
            </Link>
          }
        />
      ) : view === "board" ? (
        <Board cards={data} actions={actions} busyId={busyId} />
      ) : (
        <CardList cards={data} actions={actions} />
      )}
    </div>
  );
}
