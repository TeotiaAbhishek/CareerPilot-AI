import type { Breakdown } from "@/lib/api";
import { rate, replyRate } from "@/lib/insights";

/** Applied, replies and interviews per group, with the reply rate as a small bar. */
export function BreakdownTable({ rows, group }: { rows: Breakdown[]; group: string }) {
  if (rows.length === 0) {
    return <p className="text-sm text-muted-foreground">Nothing applied for yet.</p>;
  }
  return (
    <table className="w-full text-sm">
      <thead>
        <tr className="border-b text-left text-xs text-muted-foreground">
          <th scope="col" className="pb-2 font-medium">
            {group}
          </th>
          <th scope="col" className="pb-2 pl-3 text-right font-medium">
            Applied
          </th>
          <th scope="col" className="pb-2 pl-3 text-right font-medium">
            Interviews
          </th>
          <th scope="col" className="pb-2 pl-4 font-medium">
            Reply rate
          </th>
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => {
          const r = replyRate(row);
          return (
            <tr key={row.label} className="border-b last:border-0">
              <th scope="row" className="py-2 text-left font-normal whitespace-nowrap">
                {row.label}
              </th>
              <td className="py-2 pl-3 text-right tabular-nums">{row.applied}</td>
              <td className="py-2 pl-3 text-right tabular-nums">{row.interviews}</td>
              <td className="py-2 pl-4">
                <div className="flex items-center gap-2">
                  <div className="h-1.5 w-16 overflow-hidden rounded-full bg-muted">
                    <div className="h-full rounded-full bg-primary" style={{ width: `${(r ?? 0) * 100}%` }} />
                  </div>
                  <span className="w-9 text-right text-xs tabular-nums">{rate(r)}</span>
                </div>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
