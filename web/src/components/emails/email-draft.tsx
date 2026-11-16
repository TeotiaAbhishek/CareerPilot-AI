"use client";

import { Check, Copy, ExternalLink, Loader2, Save, X } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { DraftLineEditor, type LineContext } from "@/components/documents/draft-line";
import { Badge } from "@/components/ui/badge";
import { Button, buttonVariants } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { type Email, errorMessage, markEmailSent, saveEmail, skipEmail } from "@/lib/api";
import { openErrors } from "@/lib/documents";
import { KIND_LABEL, gmailUrl, plainText, wordCount } from "@/lib/emails";
import { cn } from "@/lib/utils";

/**
 * One email, ready for you: the address, subject and paragraphs to edit, the
 * checker's flags, and Copy / Open in Gmail / Mark sent. You send it yourself.
 */
export function EmailDraft({
  email,
  greeting,
  signOff,
  evidence,
  onChange,
}: {
  email: Email;
  greeting: string;
  signOff: string;
  evidence: Map<string, string>;
  onChange: (email: Email) => void;
}) {
  const [to, setTo] = useState(email.to_address ?? "");
  const [subject, setSubject] = useState(email.subject);
  const [paragraphs, setParagraphs] = useState(email.paragraphs);
  const [dismissed, setDismissed] = useState(email.dismissed_flags);
  const [busy, setBusy] = useState(false);

  const dirty =
    to !== (email.to_address ?? "") ||
    subject !== email.subject ||
    JSON.stringify(paragraphs) !== JSON.stringify(email.paragraphs) ||
    JSON.stringify(dismissed) !== JSON.stringify(email.dismissed_flags);
  const errors = openErrors(email.flags, dismissed);
  const ready = !dirty && errors === 0;
  const text = plainText({ paragraphs }, greeting, signOff);

  async function run(action: () => Promise<Email>, message: string) {
    setBusy(true);
    try {
      const fresh = await action();
      onChange(fresh);
      toast.success(message);
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const save = () =>
    run(async () => {
      const fresh = await saveEmail(email.id, {
        to_address: to.trim() || null,
        subject,
        paragraphs,
        dismissed_flags: dismissed,
      });
      // Keep the boxes in step with what was saved (trimmed, tidied).
      setTo(fresh.to_address ?? "");
      setSubject(fresh.subject);
      setParagraphs(fresh.paragraphs);
      setDismissed(fresh.dismissed_flags);
      return fresh;
    }, "Saved and checked again");

  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      toast.success("Copied the email. Paste it into a new message.");
    } catch {
      toast.error("Couldn't copy. Select the text and copy it yourself.");
    }
  }

  const ctx: LineContext = {
    flags: email.flags,
    dismissed,
    evidence,
    disabled: busy,
    onDismiss: (key, on) => setDismissed((d) => (on ? [...d, key] : d.filter((k) => k !== key))),
  };

  return (
    <div className="flex flex-col gap-4 rounded-lg border p-4">
      <div className="flex flex-wrap items-center gap-2">
        <p className="text-sm font-semibold">{KIND_LABEL[email.kind]}</p>
        <Badge tone="success">Ready to send</Badge>
        {errors > 0 ? (
          <Badge tone="warning">
            {errors} line{errors === 1 ? "" : "s"} to check
          </Badge>
        ) : null}
        <span className="ml-auto text-xs text-muted-foreground tabular-nums">{wordCount({ paragraphs })} words</span>
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        <label className="flex flex-col gap-1.5 text-sm">
          <span className="font-medium">To</span>
          <Input
            type="email"
            value={to}
            placeholder="Not in the ad: type the address"
            disabled={busy}
            onChange={(e) => setTo(e.target.value)}
          />
        </label>
        <label className="flex flex-col gap-1.5 text-sm">
          <span className="font-medium">Subject</span>
          <Input value={subject} disabled={busy} onChange={(e) => setSubject(e.target.value)} />
        </label>
      </div>

      <div className="flex flex-col gap-3">
        <p className="text-sm text-muted-foreground">{greeting}</p>
        {paragraphs.map((p, i) => (
          <DraftLineEditor
            key={i}
            label={`Paragraph ${i + 1}`}
            line={p}
            path={`paragraphs.${i}`}
            ctx={ctx}
            onChange={(value) => setParagraphs((all) => all.map((x, j) => (j === i ? { ...x, text: value } : x)))}
            onRemove={paragraphs.length > 1 ? () => setParagraphs((all) => all.filter((_, j) => j !== i)) : undefined}
          />
        ))}
        <p className="whitespace-pre-line text-sm text-muted-foreground">{signOff}</p>
      </div>

      {email.flags
        .filter((f) => f.path === "document")
        .map((f) => (
          <p key={f.key} className="text-xs text-muted-foreground">
            {f.message}
          </p>
        ))}

      <div className="flex flex-wrap items-center gap-2 border-t pt-4">
        {dirty ? (
          <Button size="sm" disabled={busy} onClick={save}>
            {busy ? <Loader2 className="animate-spin" /> : <Save />} Save changes
          </Button>
        ) : null}
        <a
          href={ready ? gmailUrl(to.trim() || null, subject, text) : undefined}
          target="_blank"
          rel="noreferrer"
          aria-disabled={!ready}
          className={cn(buttonVariants({ size: "sm" }), !ready && "pointer-events-none opacity-50")}
        >
          <ExternalLink /> Open in Gmail
        </a>
        <Button size="sm" variant="outline" disabled={!ready} onClick={copy}>
          <Copy /> Copy
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={busy || dirty}
          onClick={() => run(() => markEmailSent(email.id), "Marked as sent")}
        >
          <Check /> I sent it
        </Button>
        <Button
          size="sm"
          variant="ghost"
          className="ml-auto"
          disabled={busy}
          onClick={() => run(() => skipEmail(email.id), "Skipped. You can draft it again any time.")}
        >
          <X /> Skip
        </Button>
      </div>
      {!ready ? (
        <p className="text-xs text-muted-foreground">
          {dirty ? "Save your changes first." : "Fix the flagged lines, or keep them if they're true, then save."}
        </p>
      ) : null}
    </div>
  );
}
