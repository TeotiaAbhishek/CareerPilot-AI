"use client";

import { Loader2, Save } from "lucide-react";
import { type FormEvent, type ReactNode, useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { useApi } from "@/hooks/use-api";
import { type Facts, type FactsView, errorMessage, saveFacts } from "@/lib/api";
import { timeAgo } from "@/lib/format";

type TextField = {
  [K in keyof Facts]: Facts[K] extends string ? K : never;
}[keyof Facts];
type BoolField = {
  [K in keyof Facts]: Facts[K] extends boolean ? K : never;
}[keyof Facts];
type DateField = "work_rights_expiry" | "available_from";

/**
 * The facts application forms always ask for. Work rights and location are
 * also what the scoring model sees; contact details never leave the app
 * except into forms you apply with.
 */
export function FactsForm() {
  const { data, error, mutate } = useApi<FactsView>("/api/profile/facts");
  const [form, setForm] = useState<Facts | null>(null);
  const [saving, setSaving] = useState(false);

  const facts: Facts | null = form ?? data ?? null;
  const dirty = form !== null;

  function set<K extends keyof Facts>(key: K, value: Facts[K]) {
    if (!facts) return;
    setForm({ ...facts, [key]: value });
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!facts) return;
    setSaving(true);
    try {
      const saved = await saveFacts(facts);
      mutate(() => saved);
      setForm(null);
      toast.success("Facts saved");
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setSaving(false);
    }
  }

  if (error) return <p className="text-sm text-danger">{error}</p>;
  if (!facts || !data) return <Skeleton className="h-96 w-full" />;

  const text = (key: TextField, label: string, props: { type?: string; placeholder?: string; autoComplete?: string } = {}) => (
    <Field label={label} htmlFor={key}>
      <Input id={key} value={facts[key]} onChange={(e) => set(key, e.target.value)} {...props} />
    </Field>
  );
  const date = (key: DateField, label: string) => (
    <Field label={label} htmlFor={key}>
      <Input
        id={key}
        type="date"
        value={facts[key] ?? ""}
        onChange={(e) => set(key, e.target.value || null)}
      />
    </Field>
  );
  const toggle = (key: BoolField, label: string) => (
    <label className="flex items-center justify-between gap-4 rounded-md border px-3 py-2 text-sm">
      {label}
      <Switch label={label} checked={facts[key]} onCheckedChange={(on) => set(key, on)} />
    </label>
  );

  return (
    <form onSubmit={submit} className="flex flex-col gap-6">
      <Section title="Contact" description="Filled into application forms. Never sent to the scoring model.">
        {text("full_name", "Full name", { autoComplete: "name" })}
        {text("email", "Email", { type: "email", autoComplete: "email" })}
        {text("phone", "Phone", { type: "tel", autoComplete: "tel" })}
        {text("linkedin_url", "LinkedIn URL", { type: "url", placeholder: "https://www.linkedin.com/in/…" })}
        {text("website_url", "Website or GitHub", { type: "url", placeholder: "https://…" })}
        {text("street_address", "Street address", { autoComplete: "street-address" })}
      </Section>

      <Section title="Location" description="The scoring model uses this.">
        {text("city", "City")}
        {text("state", "State")}
        {text("postcode", "Postcode", { autoComplete: "postal-code" })}
        {text("country", "Country")}
        {toggle("willing_to_relocate", "Willing to relocate")}
      </Section>

      <Section title="Work rights" description="The scoring model uses this to spot hard blockers like citizenship.">
        {toggle("australian_citizen", "Australian citizen")}
        {toggle("australian_permanent_resident", "Australian permanent resident")}
        {toggle("new_zealand_citizen", "New Zealand citizen")}
        {toggle("australian_work_rights", "Has Australian work rights")}
        {text("visa_type", "Visa", { placeholder: "e.g. Temporary Graduate visa (subclass 485)" })}
        {date("work_rights_expiry", "Work rights expire")}
      </Section>

      <Section title="Applications" description="Common form questions.">
        {text("notice_period", "Notice period", { placeholder: "e.g. 2 weeks" })}
        {text("salary_expectation", "Salary expectation", { placeholder: "e.g. $100,000 – $115,000 + super" })}
        {date("available_from", "Available from")}
      </Section>

      <div className="sticky bottom-4 flex items-center justify-end gap-3 rounded-xl border bg-card/95 p-3 shadow-sm backdrop-blur">
        <span className="mr-auto text-xs text-muted-foreground">
          {dirty ? "Unsaved changes" : `Saved ${timeAgo(data.updated_at)}`}
        </span>
        {dirty ? (
          <Button variant="ghost" onClick={() => setForm(null)}>
            Discard
          </Button>
        ) : null}
        <Button type="submit" disabled={!dirty || saving}>
          {saving ? <Loader2 className="animate-spin" /> : <Save />} Save facts
        </Button>
      </div>
    </form>
  );
}

function Section({ title, description, children }: { title: string; description: string; children: ReactNode }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        <CardDescription>{description}</CardDescription>
      </CardHeader>
      <CardContent className="grid gap-4 sm:grid-cols-2">{children}</CardContent>
    </Card>
  );
}

function Field({ label, htmlFor, children }: { label: string; htmlFor: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={htmlFor} className="text-sm font-medium">
        {label}
      </label>
      {children}
    </div>
  );
}
