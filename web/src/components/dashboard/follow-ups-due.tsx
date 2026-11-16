"use client";

import { ArrowRight, Loader2, MailCheck } from "lucide-react";
import Link from "next/link";

import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import type { Email } from "@/lib/api";
import { EMAIL_STATUS, KIND_LABEL, isDrafting } from "@/lib/emails";
import { timeAgo } from "@/lib/format";

/** Follow-ups and thank-yous that are due, oldest first. */
export function FollowUpsDue() {
  const { data, error } = useApi<Email[]>("/api/emails/due");

  return (
    <Card>
      <CardHeader>
        <CardTitle>Follow-ups due</CardTitle>
        <CardDescription>Drafted for you. Open one, check it, and send it from Gmail.</CardDescription>
      </CardHeader>
      <CardContent>
        {error ? (
          <p className="text-sm text-danger">{error}</p>
        ) : !data ? (
          <Skeleton className="h-16 w-full" />
        ) : data.length === 0 ? (
          <p className="flex items-center gap-2 text-sm text-muted-foreground">
            <MailCheck className="size-4" aria-hidden /> Nothing due. Follow-ups appear 7 days after you apply.
          </p>
        ) : (
          <ul className="flex flex-col divide-y">
            {data.map((email) => (
              <li key={email.id}>
                <Link
                  href={`/jobs/${encodeURIComponent(email.job_id)}#emails`}
                  className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2.5 text-sm hover:text-primary"
                >
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-medium">
                      {email.title}
                      {email.company ? <span className="text-muted-foreground"> · {email.company}</span> : null}
                    </span>
                    <span className="text-xs text-muted-foreground">
                      {KIND_LABEL[email.kind]} · due {timeAgo(email.due_at)}
                    </span>
                  </span>
                  <Badge tone={EMAIL_STATUS[email.status].tone}>
                    {isDrafting(email.status) ? <Loader2 className="size-3 animate-spin" aria-hidden /> : null}
                    {EMAIL_STATUS[email.status].label}
                  </Badge>
                  <ArrowRight className="size-4 text-muted-foreground" aria-hidden />
                </Link>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}
