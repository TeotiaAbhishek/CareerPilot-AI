"use client";

import { Loader2, Mail, RotateCcw, Sparkles } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import { type Email, type EmailKind, type JobEmails, draftEmail, errorMessage } from "@/lib/api";
import { EMAIL_STATUS, KIND_LABEL, isDrafting } from "@/lib/emails";
import { formatDate } from "@/lib/format";

import { EmailDraft } from "./email-draft";

const POLL_MS = 4000;

/** Emails on a job page: an application email if the ad has an address, follow-ups and a thank-you. */
export function EmailPanel({ jobId }: { jobId: string }) {
  const { data, error, reload, mutate } = useApi<JobEmails>(`/api/jobs/${encodeURIComponent(jobId)}/emails`);
  const [busy, setBusy] = useState<EmailKind | null>(null);

  const drafting = data?.emails.some((e) => isDrafting(e.status)) ?? false;
  useEffect(() => {
    if (!drafting) return;
    const timer = setInterval(reload, POLL_MS);
    return () => clearInterval(timer);
  }, [drafting, reload]);

  const evidence = useMemo(() => new Map((data?.evidence ?? []).map((e) => [e.id, e.text])), [data?.evidence]);

  async function draft(kind: EmailKind) {
    setBusy(kind);
    try {
      await draftEmail(jobId, kind);
      toast.success(`Drafting the ${KIND_LABEL[kind].toLowerCase()}. It takes about a minute.`);
      reload();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(null);
    }
  }

  const replace = (email: Email) =>
    mutate((d) => ({ ...d, emails: d.emails.map((e) => (e.id === email.id ? email : e)) }));

  const application = data?.emails.find((e) => e.kind === "application");

  return (
    <section id="emails" aria-labelledby="emails-title" className="scroll-mt-20 border-t p-6">
      <h3 id="emails-title" className="text-sm font-semibold">
        Emails
      </h3>

      {error ? (
        <p className="mt-3 text-sm text-danger">{error}</p>
      ) : !data ? (
        <Skeleton className="mt-3 h-14 w-full" />
      ) : (
        <div className="mt-3 flex flex-col gap-3">
          {data.address && (!application || application.status === "cancelled") ? (
            <div className="flex flex-wrap items-center gap-3 rounded-lg bg-muted p-3 text-sm">
              <Mail className="size-4 text-muted-foreground" aria-hidden />
              <span className="min-w-0 flex-1">
                This ad lists <span className="font-medium">{data.address}</span>.
              </span>
              <Button size="sm" variant="outline" disabled={busy !== null} onClick={() => draft("application")}>
                {busy === "application" ? <Loader2 className="animate-spin" /> : <Sparkles />} Draft an application
                email
              </Button>
            </div>
          ) : null}

          {data.emails.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              Follow-ups are drafted 7 and 14 days after you apply, and a thank-you when you move the card to
              Interview. You send them yourself.
            </p>
          ) : null}

          {data.emails.map((email) =>
            email.status === "ready" ? (
              <EmailDraft
                key={`${email.id}:${email.updated_at}`}
                email={email}
                greeting={data.greeting}
                signOff={data.sign_off}
                evidence={evidence}
                onChange={replace}
              />
            ) : (
              <div key={email.id} className="flex flex-wrap items-center gap-x-3 gap-y-2 rounded-lg border p-3 text-sm">
                {isDrafting(email.status) ? (
                  <Loader2 className="size-4 animate-spin text-primary" aria-hidden />
                ) : (
                  <Mail className="size-4 text-muted-foreground" aria-hidden />
                )}
                <span className="font-medium">{KIND_LABEL[email.kind]}</span>
                <Badge tone={EMAIL_STATUS[email.status].tone}>{EMAIL_STATUS[email.status].label}</Badge>
                <span className="min-w-0 flex-1 truncate text-muted-foreground">
                  {email.status === "sent"
                    ? `Sent ${formatDate(email.sent_at)}${email.to_address ? ` to ${email.to_address}` : ""}`
                    : email.status === "failed"
                      ? email.error
                      : isDrafting(email.status)
                        ? "Writing it now"
                        : null}
                </span>
                {email.status === "failed" || email.status === "cancelled" ? (
                  <Button size="sm" variant="ghost" disabled={busy !== null} onClick={() => draft(email.kind)}>
                    {busy === email.kind ? <Loader2 className="animate-spin" /> : <RotateCcw />} Draft again
                  </Button>
                ) : null}
              </div>
            ),
          )}
        </div>
      )}
    </section>
  );
}
