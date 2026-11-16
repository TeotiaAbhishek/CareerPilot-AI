"use client";

import { CircleCheck, Mail, TriangleAlert, Unplug } from "lucide-react";
import { useSearchParams } from "next/navigation";
import { useEffect, useState } from "react";
import { toast } from "sonner";

import { Button, buttonVariants } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import { type MailStatus, disconnectMail, errorMessage, mailConnectUrl } from "@/lib/api";

const RESULT: Record<string, [ok: boolean, message: string]> = {
  connected: [true, "Gmail connected. The agent can now read the codes employers email you."],
  cancelled: [false, "Gmail wasn't connected: you cancelled on Google's page."],
  failed: [false, "Gmail wasn't connected. Check the API terminal for the reason."],
};

/** Gmail (read-only), so the agent can read sign-up codes and verification links. */
export function EmailSettings() {
  const mail = useApi<MailStatus>("/api/mail");
  const params = useSearchParams();
  const [busy, setBusy] = useState(false);
  const result = params.get("gmail");

  // Google sends you back to /settings?gmail=connected|cancelled|failed.
  useEffect(() => {
    const shown = result ? RESULT[result] : undefined;
    if (!shown) return;
    if (shown[0]) toast.success(shown[1]);
    else toast.error(shown[1]);
    window.history.replaceState(null, "", "/settings");
  }, [result]);

  async function disconnect() {
    setBusy(true);
    try {
      await disconnectMail();
      mail.reload();
      toast.success("Gmail disconnected. The saved token was deleted.");
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const m = mail.data;

  return (
    <Card>
      <CardHeader>
        <CardTitle>Email codes</CardTitle>
        <CardDescription>
          Employer sites often email a code when you sign up. With Gmail connected (read-only), the agent reads the code
          from the last day&apos;s mail and types it in. It can&apos;t send, delete or change anything.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 text-sm">
        {mail.error ? (
          <p className="text-danger">{mail.error}</p>
        ) : !m ? (
          <Skeleton className="h-16 w-full" />
        ) : !m.configured ? (
          <div className="flex gap-2 rounded-lg bg-muted p-3 text-warning">
            <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
            <p>
              Not set up yet. Add <span className="font-mono text-xs">GMAIL_CLIENT_ID</span> and{" "}
              <span className="font-mono text-xs">GMAIL_CLIENT_SECRET</span> to backend/.env and restart the API. Until
              then the agent stops and asks you for codes. Practice runs don&apos;t need Gmail.
            </p>
          </div>
        ) : m.account ? (
          <div className="flex flex-wrap items-center justify-between gap-3">
            <p className="flex items-center gap-2">
              <CircleCheck className="size-4 text-success" aria-hidden /> Connected as {m.account}
            </p>
            <Button variant="outline" disabled={busy} onClick={disconnect}>
              <Unplug /> Disconnect
            </Button>
          </div>
        ) : (
          <div>
            <a href={mailConnectUrl()} className={buttonVariants()}>
              <Mail /> Connect Gmail (read-only)
            </a>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
