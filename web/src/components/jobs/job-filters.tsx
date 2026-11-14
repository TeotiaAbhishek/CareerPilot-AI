"use client";

import { Search, X } from "lucide-react";
import { useEffect, useState } from "react";

import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import type { Facet } from "@/lib/api";
import type { JobFilters, JobStatus } from "@/lib/jobs-query";
import { cn } from "@/lib/utils";

const STATUS_TABS: { value: JobStatus; label: string }[] = [
  { value: "all", label: "All" },
  { value: "new", label: "New" },
  { value: "saved", label: "Saved" },
  { value: "hidden", label: "Hidden" },
];

const POSTED_OPTIONS = [
  { value: "", label: "Any time" },
  { value: "1", label: "Last 24 hours" },
  { value: "3", label: "Last 3 days" },
  { value: "7", label: "Last 7 days" },
  { value: "14", label: "Last 14 days" },
  { value: "30", label: "Last 30 days" },
];

export function JobFiltersBar({
  filters,
  workTypes,
  onChange,
}: {
  filters: JobFilters;
  workTypes: Facet[];
  onChange: (patch: Partial<JobFilters>) => void;
}) {
  // Typing updates this box at once; the URL (and the API call) waits 300 ms
  // after the last key press, so we don't search on every letter.
  const [text, setText] = useState(filters.q);
  const [lastQ, setLastQ] = useState(filters.q);

  // If the URL changes from elsewhere (back button, a link), follow it.
  if (filters.q !== lastQ) {
    setLastQ(filters.q);
    setText(filters.q);
  }

  useEffect(() => {
    if (text === filters.q) return;
    const id = setTimeout(() => onChange({ q: text, page: 1 }), 300);
    return () => clearTimeout(id);
  }, [text, filters.q, onChange]);

  return (
    <div className="flex flex-col gap-3">
      <div className="relative">
        <Search
          className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
          aria-hidden
        />
        <Input
          type="search"
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder="Search title, company or description…"
          aria-label="Search jobs"
          className="pl-9 pr-9"
        />
        {text ? (
          <button
            type="button"
            onClick={() => setText("")}
            aria-label="Clear search"
            className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-muted-foreground hover:text-foreground"
          >
            <X className="size-4" />
          </button>
        ) : null}
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <div role="tablist" aria-label="Status" className="inline-flex rounded-md border bg-muted p-0.5">
          {STATUS_TABS.map((tab) => (
            <button
              key={tab.value}
              type="button"
              role="tab"
              aria-selected={filters.status === tab.value}
              onClick={() => onChange({ status: tab.value, page: 1 })}
              className={cn(
                "rounded px-3 py-1 text-sm font-medium transition-colors",
                filters.status === tab.value
                  ? "bg-card text-foreground shadow-xs"
                  : "text-muted-foreground hover:text-foreground",
              )}
            >
              {tab.label}
            </button>
          ))}
        </div>

        <Select
          aria-label="Work type"
          value={filters.workType}
          onChange={(e) => onChange({ workType: e.target.value, page: 1 })}
          className="w-40"
        >
          <option value="">All work types</option>
          {workTypes.map((w) => (
            <option key={w.value} value={w.value}>
              {w.value} ({w.count})
            </option>
          ))}
        </Select>

        <Select
          aria-label="Posted"
          value={filters.postedWithinDays?.toString() ?? ""}
          onChange={(e) =>
            onChange({ postedWithinDays: e.target.value ? Number(e.target.value) : null, page: 1 })
          }
          className="w-36"
        >
          {POSTED_OPTIONS.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </Select>

        <Select
          aria-label="Sort"
          value={filters.sort}
          onChange={(e) => onChange({ sort: e.target.value as JobFilters["sort"], page: 1 })}
          className="w-36"
        >
          <option value="newest">Newest first</option>
          <option value="oldest">Oldest first</option>
          <option value="best_fit">Best fit</option>
          <option value="relevance" disabled={!filters.q}>
            Best text match
          </option>
        </Select>

        <label className="flex items-center gap-2 text-sm">
          <Switch
            label="Relevant only"
            checked={filters.relevantOnly}
            onCheckedChange={(relevantOnly) => onChange({ relevantOnly, page: 1 })}
          />
          Relevant only
        </label>
      </div>
    </div>
  );
}
