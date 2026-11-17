"use client";

import { Check, Cloud, KeyRound } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Select } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import { type LlmUsage, type ProviderUsage, errorMessage, setMonthlyCap } from "@/lib/api";
import { dollars } from "@/lib/scoring";

const CAPS = [0, 2, 5, 10, 20, 50];
const NAMES: Record<ProviderUsage["provider"], { name: string; key: string; console: string }> = {
  claude: { name: "Claude", key: "ANTHROPIC_API_KEY", console: "https://console.anthropic.com" },
  openai: { name: "OpenAI", key: "OPENAI_API_KEY", console: "https://platform.openai.com" },
};

/**
 * Claude and OpenAI: whether their key is set, what they've cost this month,
 * and the monthly cap. Their models then show up in every model picker.
 */
export function CloudSettings() {
  const { data, error, mutate } = useApi<LlmUsage>("/api/llm/usage");
  const [saving, setSaving] = useState<string | null>(null);

  async function saveCap(provider: ProviderUsage["provider"], cap: number) {
    setSaving(provider);
    try {
      const updated = await setMonthlyCap(provider, cap);
      mutate(() => updated);
      toast.success(
        cap === 0 ? `${NAMES[provider].name} is off` : `${NAMES[provider].name}: at most ${dollars(cap)} a month`,
      );
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setSaving(null);
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Cloud models</CardTitle>
        <CardDescription>
          Claude and OpenAI models appear in every model picker below once their API key is in{" "}
          <code className="font-mono">backend/.env</code>. Your contact details are never sent. Past a
          provider&apos;s monthly cap, its calls go to your local apply model instead.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {error ? (
          <p className="text-sm text-danger">{error}</p>
        ) : !data ? (
          <Skeleton className="h-36 w-full" />
        ) : (
          <>
            <div className="grid gap-4 sm:grid-cols-2">
              {data.providers.map((p) => (
                <ProviderCard
                  key={p.provider}
                  usage={p}
                  saving={saving === p.provider}
                  onCap={(cap) => saveCap(p.provider, cap)}
                />
              ))}
            </div>
            <details className="text-sm">
              <summary className="cursor-pointer text-muted-foreground hover:text-foreground">
                Prices per million tokens
              </summary>
              <table className="mt-2 w-full max-w-md text-left">
                <thead className="text-xs text-muted-foreground">
                  <tr>
                    <th className="py-1 pr-4 font-medium">Model</th>
                    <th className="py-1 pr-4 text-right font-medium">Input</th>
                    <th className="py-1 text-right font-medium">Output</th>
                  </tr>
                </thead>
                <tbody>
                  {data.models.map((m) => (
                    <tr key={m.id}>
                      <td className="py-1 pr-4 font-mono text-xs">{m.id}</td>
                      <td className="py-1 pr-4 text-right tabular-nums">{dollars(m.input_per_mtok)}</td>
                      <td className="py-1 text-right tabular-nums">{dollars(m.output_per_mtok)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </details>
          </>
        )}
      </CardContent>
    </Card>
  );
}

function ProviderCard({
  usage,
  saving,
  onCap,
}: {
  usage: ProviderUsage;
  saving: boolean;
  onCap: (cap: number) => void;
}) {
  const info = NAMES[usage.provider];
  const share = usage.monthly_cap_usd > 0 ? Math.min(1, usage.spent_this_month_usd / usage.monthly_cap_usd) : 1;
  const reached = usage.spent_this_month_usd >= usage.monthly_cap_usd;

  return (
    <div className="flex flex-col gap-3 rounded-lg border p-3">
      <div className="flex items-center gap-2">
        <Cloud className="size-4 text-muted-foreground" aria-hidden />
        <span className="font-medium">{info.name}</span>
        {usage.key_set ? (
          <Badge tone="success" className="ml-auto">
            <Check className="size-3" aria-hidden /> Key set
          </Badge>
        ) : (
          <Badge className="ml-auto">
            <KeyRound className="size-3" aria-hidden /> No key
          </Badge>
        )}
      </div>

      {usage.key_set ? (
        <>
          <div>
            <div className="flex justify-between text-sm tabular-nums">
              <span>
                {dollars(usage.spent_this_month_usd)} of {dollars(usage.monthly_cap_usd)} this month
              </span>
              <span className="text-muted-foreground">{usage.calls_this_month} calls</span>
            </div>
            <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-muted">
              <div
                className={reached ? "h-full rounded-full bg-warning" : "h-full rounded-full bg-primary"}
                style={{ width: `${share * 100}%` }}
              />
            </div>
            {reached ? (
              <p className="mt-1.5 text-xs text-warning">Cap reached: its tasks use your local model until next month.</p>
            ) : null}
          </div>
          <label className="flex items-center gap-2 text-sm">
            <span className="font-medium">Monthly cap</span>
            <Select
              aria-label={`${info.name} monthly cap`}
              value={usage.monthly_cap_usd}
              disabled={saving}
              onChange={(e) => onCap(Number(e.target.value))}
              className="w-32"
            >
              {Array.from(new Set([...CAPS, usage.monthly_cap_usd]))
                .sort((a, b) => a - b)
                .map((cap) => (
                  <option key={cap} value={cap}>
                    {cap === 0 ? "Off" : dollars(cap)}
                  </option>
                ))}
            </Select>
          </label>
        </>
      ) : (
        <p className="text-sm text-muted-foreground">
          Get a key at{" "}
          <a href={info.console} target="_blank" rel="noreferrer" className="text-primary underline-offset-4 hover:underline">
            {info.console.replace("https://", "")}
          </a>
          , add <code className="font-mono text-xs">{info.key}=…</code> to{" "}
          <code className="font-mono text-xs">backend/.env</code> and restart the API.
        </p>
      )}
    </div>
  );
}
