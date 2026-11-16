"use client";

import { useState } from "react";
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";

import type { DayCount } from "@/lib/api";
import { dayLabel } from "@/lib/insights";
import { cn } from "@/lib/utils";

const AXIS = { fontSize: 12, fill: "var(--color-muted-foreground)" };

/** Applications per day (last 60 days) or per week (last 12 weeks), as bars. */
export function ActivityChart({ perDay, perWeek }: { perDay: DayCount[]; perWeek: DayCount[] }) {
  const [by, setBy] = useState<"day" | "week">("day");
  const rows = (by === "day" ? perDay : perWeek).map((r) => ({
    ...r,
    label: by === "day" ? dayLabel(r.day) : `Week of ${dayLabel(r.day)}`,
  }));
  const total = rows.reduce((sum, r) => sum + r.count, 0);

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-sm text-muted-foreground tabular-nums">
          {total} in the last {by === "day" ? "60 days" : "12 weeks"}
        </p>
        <div role="tablist" aria-label="Group by" className="flex gap-1 rounded-lg border p-0.5">
          {(["day", "week"] as const).map((option) => (
            <button
              key={option}
              role="tab"
              aria-selected={by === option}
              onClick={() => setBy(option)}
              className={cn(
                "rounded-md px-2.5 py-1 text-xs font-medium",
                by === option ? "bg-primary text-primary-foreground" : "text-muted-foreground hover:bg-muted",
              )}
            >
              Per {option}
            </button>
          ))}
        </div>
      </div>

      <div className="h-56" aria-hidden>
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={rows} margin={{ top: 4, right: 4, bottom: 0, left: -24 }}>
            <CartesianGrid vertical={false} stroke="var(--color-border)" />
            <XAxis
              dataKey="label"
              tick={AXIS}
              tickLine={false}
              axisLine={{ stroke: "var(--color-border)" }}
              interval="preserveStartEnd"
              minTickGap={24}
              tickFormatter={(l: string) => l.replace("Week of ", "")}
            />
            <YAxis allowDecimals={false} tick={AXIS} tickLine={false} axisLine={false} width={48} />
            <Tooltip
              cursor={{ fill: "var(--color-muted)" }}
              contentStyle={{
                background: "var(--color-card)",
                border: "1px solid var(--color-border)",
                borderRadius: 8,
                fontSize: 12,
              }}
              labelStyle={{ color: "var(--color-foreground)", fontWeight: 600 }}
              itemStyle={{ color: "var(--color-foreground)" }}
              formatter={(value) => [value, "Applied"]}
            />
            <Bar dataKey="count" fill="var(--color-primary)" radius={[4, 4, 0, 0]} maxBarSize={by === "day" ? 10 : 28} />
          </BarChart>
        </ResponsiveContainer>
      </div>

      {/* The same numbers for screen readers. */}
      <table className="sr-only">
        <caption>Applications per {by}</caption>
        <tbody>
          {rows.map((r) => (
            <tr key={r.day}>
              <th scope="row">{r.label}</th>
              <td>{r.count}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
