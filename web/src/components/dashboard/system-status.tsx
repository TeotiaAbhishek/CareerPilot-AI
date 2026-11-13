"use client";

import { Database, RefreshCw, Server } from "lucide-react";
import { useEffect, useState } from "react";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { type Health, getHealth } from "@/lib/api";

type State =
  | { kind: "loading" }
  | { kind: "ok"; health: Health }
  | { kind: "error"; message: string };

const POLL_MS = 15_000;

/** Ask the API for its health and turn the answer (or the failure) into UI state. */
async function loadStatus(): Promise<State> {
  try {
    return { kind: "ok", health: await getHealth({ cache: "no-store" }) };
  } catch (error) {
    return {
      kind: "error",
      message: error instanceof Error ? error.message : "Unknown error",
    };
  }
}

/** Live status of the API and the database, checked every 15 seconds. */
export function SystemStatus() {
  const [state, setState] = useState<State>({ kind: "loading" });
  // Bumping this restarts the effect below, which checks immediately.
  const [refreshKey, setRefreshKey] = useState(0);

  useEffect(() => {
    let cancelled = false;

    async function poll() {
      const next = await loadStatus();
      // Ignore answers that arrive after the component is gone.
      if (!cancelled) setState(next);
    }

    void poll();
    const id = setInterval(poll, POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [refreshKey]);

  return (
    <Card>
      <CardHeader className="flex-row items-start justify-between">
        <div className="flex flex-col gap-1">
          <CardTitle>System status</CardTitle>
          <CardDescription>Checked every 15 seconds</CardDescription>
        </div>
        <Button variant="ghost" size="icon" aria-label="Check now" onClick={() => setRefreshKey((k) => k + 1)}>
          <RefreshCw />
        </Button>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {state.kind === "loading" ? (
          <>
            <Skeleton className="h-6 w-full" />
            <Skeleton className="h-6 w-full" />
          </>
        ) : (
          <>
            <StatusRow
              icon={Server}
              label="API"
              ok={state.kind === "ok"}
              detail={state.kind === "ok" ? `v${state.health.version}` : state.message}
            />
            <StatusRow
              icon={Database}
              label="Database"
              ok={state.kind === "ok" && state.health.database === "ok"}
              detail={
                state.kind === "ok"
                  ? state.health.database === "ok"
                    ? "Connected"
                    : "Unreachable"
                  : "Unknown"
              }
            />
          </>
        )}
      </CardContent>
    </Card>
  );
}

function StatusRow({
  icon: Icon,
  label,
  ok,
  detail,
}: {
  icon: typeof Server;
  label: string;
  ok: boolean;
  detail: string;
}) {
  return (
    <div className="flex items-center justify-between gap-3 text-sm">
      <span className="flex items-center gap-2">
        <Icon className="size-4 text-muted-foreground" aria-hidden />
        {label}
      </span>
      <Badge tone={ok ? "success" : "danger"} title={detail}>
        <span className={ok ? "size-1.5 rounded-full bg-success" : "size-1.5 rounded-full bg-danger"} />
        {ok ? "Online" : "Down"}
      </Badge>
    </div>
  );
}
