"use client";

import {
  type CollisionDetection,
  DndContext,
  type DragEndEvent,
  DragOverlay,
  type DragStartEvent,
  KeyboardSensor,
  MouseSensor,
  pointerWithin,
  rectIntersection,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import { type ReactNode, useState } from "react";

import { Badge } from "@/components/ui/badge";
import type { Card, Column } from "@/lib/api";
import { COLUMNS, dropStage, groupByColumn } from "@/lib/tracker";
import { cn } from "@/lib/utils";

import type { CardActions } from "./card-menu";
import { TrackerCard } from "./tracker-card";

/** Drop where the mouse is; with the keyboard (no mouse), where the card is. */
const dropTarget: CollisionDetection = (args) => {
  const under = pointerWithin(args);
  return under.length > 0 ? under : rectIntersection(args);
};

/**
 * The pipeline as columns. Drag a card to Submitted and beyond (or use its
 * "…" menu). The first three columns follow the agent, so you can't drop there.
 */
export function Board({ cards, actions, busyId }: { cards: Card[]; actions: CardActions; busyId: string | null }) {
  const groups = groupByColumn(cards);
  const [dragged, setDragged] = useState<Card | null>(null);
  const sensors = useSensors(
    // Mouse only, after a small move so clicks on links and buttons still work.
    // On a phone, swiping scrolls the board and the card menu moves cards.
    useSensor(MouseSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor),
  );

  function onDragStart(event: DragStartEvent) {
    setDragged(cards.find((c) => c.job_id === event.active.id) ?? null);
  }

  function onDragEnd(event: DragEndEvent) {
    setDragged(null);
    const card = cards.find((c) => c.job_id === event.active.id);
    const column = event.over?.id as Column | undefined;
    if (!card || !column) return;
    const stage = dropStage(card, column);
    if (stage) actions.move(card, stage);
  }

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={dropTarget} onDragStart={onDragStart} onDragEnd={onDragEnd} onDragCancel={() => setDragged(null)}>
      <div className="-mx-4 overflow-x-auto px-4 pb-4 sm:mx-0 sm:px-0">
        <div className="flex gap-3">
          {COLUMNS.map((column) => (
            <BoardColumn
              key={column.id}
              id={column.id}
              label={column.label}
              tone={column.tone}
              count={groups[column.id].length}
              droppable={!!column.stage}
              active={dragged ? dropStage(dragged, column.id) !== null : false}
            >
              {groups[column.id].map((card) => (
                <DraggableCard key={card.job_id} card={card}>
                  <TrackerCard card={card} actions={actions} busy={busyId === card.job_id} />
                </DraggableCard>
              ))}
              {groups[column.id].length === 0 ? (
                <p className="rounded-lg border border-dashed p-3 text-center text-xs text-muted-foreground">
                  {column.stage ? "Drop a card here" : "Nothing here"}
                </p>
              ) : null}
            </BoardColumn>
          ))}
        </div>
      </div>
      <DragOverlay>{dragged ? <TrackerCard card={dragged} actions={actions} dragging /> : null}</DragOverlay>
    </DndContext>
  );
}

function BoardColumn({
  id,
  label,
  tone,
  count,
  droppable,
  active,
  children,
}: {
  id: Column;
  label: string;
  tone: "neutral" | "primary" | "success" | "warning" | "danger";
  count: number;
  droppable: boolean;
  active: boolean;
  children: ReactNode;
}) {
  const { setNodeRef, isOver } = useDroppable({ id, disabled: !droppable });
  return (
    <section
      ref={setNodeRef}
      aria-label={label}
      className={cn(
        "flex w-64 shrink-0 flex-col gap-2 rounded-xl border bg-muted/40 p-2 transition-colors",
        active && "border-primary/40 border-dashed",
        isOver && active && "bg-primary/10",
      )}
    >
      <h2 className="flex items-center justify-between px-1 pt-1 text-sm font-semibold">
        {label}
        <Badge tone={tone} className="py-0 tabular-nums">
          {count}
        </Badge>
      </h2>
      {children}
    </section>
  );
}

function DraggableCard({ card, children }: { card: Card; children: ReactNode }) {
  const { setNodeRef, listeners, attributes, isDragging } = useDraggable({ id: card.job_id });
  return (
    <div
      ref={setNodeRef}
      {...listeners}
      {...attributes}
      aria-roledescription="draggable card"
      aria-label={`${card.title}${card.company ? ` at ${card.company}` : ""}`}
      className={cn(
        "cursor-grab rounded-lg outline-none focus-visible:ring-2 focus-visible:ring-ring",
        isDragging && "opacity-40",
      )}
    >
      {children}
    </div>
  );
}
