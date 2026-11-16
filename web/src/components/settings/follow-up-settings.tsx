"use client";

import { useState } from "react";
import { toast } from "sonner";

import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { useApi } from "@/hooks/use-api";
import { type Settings, errorMessage, updateSettings } from "@/lib/api";

/** When follow-up emails are drafted. */
export function FollowUpSettings() {
  const { data, error, mutate } = useApi<Settings>("/api/settings");
  const [busy, setBusy] = useState(false);

  async function toggle(on: boolean) {
    setBusy(true);
    try {
      const fresh = await updateSettings({ follow_up_second: on });
      mutate(() => fresh);
      toast.success(on ? "Second follow-up on" : "Second follow-up off");
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Follow-up emails</CardTitle>
        <CardDescription>
          Drafted for applications in Submitted: one 7 days after you apply, and a thank-you when you move a card to
          Interview. They stop once the card moves on. You send them yourself.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {error ? (
          <p className="text-sm text-danger">{error}</p>
        ) : !data ? (
          <Skeleton className="h-10 w-full" />
        ) : (
          <div className="flex items-start justify-between gap-4">
            <div>
              <p className="text-sm font-medium">A second follow-up at 14 days</p>
              <p className="text-sm text-muted-foreground">If the first one gets no reply.</p>
            </div>
            <Switch
              label="A second follow-up at 14 days"
              checked={data.follow_up_second}
              disabled={busy}
              onCheckedChange={toggle}
            />
          </div>
        )}
      </CardContent>
    </Card>
  );
}
