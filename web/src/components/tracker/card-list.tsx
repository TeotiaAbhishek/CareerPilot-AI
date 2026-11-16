"use client";

import Link from "next/link";
import { useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import type { Card, Column } from "@/lib/api";
import { formatDate, timeAgo } from "@/lib/format";
import { COLUMNS, type ListFilters, NO_FILTERS, columnInfo, filterCards } from "@/lib/tracker";

import { type CardActions, CardMenu } from "./card-menu";

const RANGES = [
  { value: "", label: "Any time" },
  { value: "7", label: "Last 7 days" },
  { value: "30", label: "Last 30 days" },
  { value: "90", label: "Last 90 days" },
];

/** Every card as rows, with filters for the stage, the company and the date. */
export function CardList({ cards, actions }: { cards: Card[]; actions: CardActions }) {
  const [filters, setFilters] = useState<ListFilters>(NO_FILTERS);
  const rows = filterCards(cards, filters);
  const set = (patch: Partial<ListFilters>) => setFilters((f) => ({ ...f, ...patch }));

  return (
    <div className="flex flex-col gap-4">
      <div className="grid gap-3 sm:grid-cols-3">
        <label className="flex flex-col gap-1.5 text-sm">
          <span className="font-medium">Stage</span>
          <Select value={filters.column} onChange={(e) => set({ column: e.target.value as Column | "all" })}>
            <option value="all">Every stage</option>
            {COLUMNS.map((c) => (
              <option key={c.id} value={c.id}>
                {c.label}
              </option>
            ))}
          </Select>
        </label>
        <label className="flex flex-col gap-1.5 text-sm">
          <span className="font-medium">Company</span>
          <Input
            value={filters.company}
            placeholder="Any company"
            onChange={(e) => set({ company: e.target.value })}
          />
        </label>
        <label className="flex flex-col gap-1.5 text-sm">
          <span className="font-medium">Last changed</span>
          <Select
            value={filters.days === null ? "" : String(filters.days)}
            onChange={(e) => set({ days: e.target.value ? Number(e.target.value) : null })}
          >
            {RANGES.map((r) => (
              <option key={r.value} value={r.value}>
                {r.label}
              </option>
            ))}
          </Select>
        </label>
      </div>

      <p className="text-sm text-muted-foreground tabular-nums">
        {rows.length} of {cards.length}
      </p>

      {rows.length === 0 ? (
        <p className="rounded-xl border border-dashed p-6 text-center text-sm text-muted-foreground">
          Nothing matches these filters.
        </p>
      ) : (
        <ul className="flex flex-col divide-y rounded-xl border bg-card">
          {rows.map((card) => {
            const column = columnInfo(card.column);
            return (
              <li key={card.job_id} className="flex flex-wrap items-center gap-x-4 gap-y-2 p-4">
                <div className="min-w-0 flex-1 basis-60">
                  <Link
                    href={`/jobs/${encodeURIComponent(card.job_id)}`}
                    className="block truncate font-medium hover:text-primary hover:underline"
                  >
                    {card.title}
                    {card.company ? <span className="text-muted-foreground"> · {card.company}</span> : null}
                  </Link>
                  <p className="truncate text-sm text-muted-foreground">
                    {card.application_id ? (
                      <Link href={`/applications/${card.application_id}`} className="hover:text-primary hover:underline">
                        {card.detail}
                      </Link>
                    ) : (
                      card.detail
                    )}
                  </p>
                </div>
                <div className="flex items-center gap-3">
                  <Badge tone={column.tone}>{column.label}</Badge>
                  <span className="w-28 text-right text-xs text-muted-foreground tabular-nums">
                    {card.applied_at ? `Applied ${formatDate(card.applied_at)}` : timeAgo(card.updated_at)}
                  </span>
                  <CardMenu card={card} actions={actions} />
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
