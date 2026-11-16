"use client";

import {
  Copy,
  ExternalLink,
  Eye,
  EyeOff,
  KeyRound,
  Lock,
  Pencil,
  Plus,
  ShieldCheck,
  Wand2,
} from "lucide-react";
import { type FormEvent, useEffect, useState } from "react";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import {
  type Credential,
  type CredentialInput,
  type VaultStatus,
  createCredential,
  deleteCredential,
  errorMessage,
  generatePassword,
  revealPassword,
  updateCredential,
} from "@/lib/api";
import { timeAgo } from "@/lib/format";

import { ConfirmDelete } from "./answers-manager";
import { UsualLoginCard } from "./usual-login";

/** How long a revealed password stays on screen. */
const REVEAL_MS = 20_000;

/**
 * Saved logins, one per site. Passwords are encrypted in the database and only
 * decrypted one at a time when you reveal or copy them (each time is logged).
 */
export function VaultManager() {
  const status = useApi<VaultStatus>("/api/vault/status");
  const list = useApi<Credential[]>("/api/vault");
  const [editing, setEditing] = useState<Credential | "new" | null>(null);

  function refresh() {
    list.reload();
    status.reload();
  }

  if (status.error || list.error) {
    return <p className="text-sm text-danger">{status.error ?? list.error}</p>;
  }
  if (!status.data || !list.data) return <Skeleton className="h-64 w-full" />;

  if (!status.data.unlocked) {
    return (
      <EmptyState
        icon={Lock}
        title="The vault is locked"
        description="Create a key once, then restart the API: cd ~/code/careerpilot-ai/backend && cargo run -- vault-init"
      />
    );
  }

  return (
    <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_18rem]">
      <div className="flex flex-col gap-4">
        <div className="flex flex-wrap items-center gap-3">
          <p className="mr-auto flex items-center gap-2 text-sm text-muted-foreground">
            <ShieldCheck className="size-4 text-success" aria-hidden />
            Encrypted with AES-256-GCM. The key is in <code className="font-mono">backend/.env</code>, not the database.
          </p>
          <Button onClick={() => setEditing("new")} disabled={editing !== null}>
            <Plus /> Add login
          </Button>
        </div>

        {editing === "new" ? (
          <CredentialForm
            onCancel={() => setEditing(null)}
            onSave={async (input) => {
              await createCredential(input);
              toast.success(`Saved the login for ${input.site}`);
              setEditing(null);
              refresh();
            }}
          />
        ) : null}

        {list.data.length === 0 && editing === null ? (
          <EmptyState
            icon={KeyRound}
            title="No logins yet"
            description="Add your SEEK login, and any job sites you use. The apply agent signs in with these, and saves new accounts it creates."
          />
        ) : (
          <ul className="flex flex-col gap-3">
            {list.data.map((c) =>
              editing !== "new" && editing?.id === c.id ? (
                <li key={c.id}>
                  <CredentialForm
                    initial={c}
                    onCancel={() => setEditing(null)}
                    onSave={async (input) => {
                      await updateCredential(c.id, input);
                      toast.success("Login updated");
                      setEditing(null);
                      refresh();
                    }}
                  />
                </li>
              ) : (
                <CredentialRow
                  key={c.id}
                  credential={c}
                  onEdit={() => setEditing(c)}
                  onChanged={refresh}
                />
              ),
            )}
          </ul>
        )}
      </div>

      <div className="flex flex-col gap-6">
        <UsualLoginCard />
        <Card className="h-fit">
          <CardHeader>
            <CardTitle>Recent access</CardTitle>
            <CardDescription>Every reveal, copy and change.</CardDescription>
          </CardHeader>
          <CardContent>
            {status.data.recent_access.length === 0 ? (
              <p className="text-sm text-muted-foreground">Nothing yet.</p>
            ) : (
              <ul className="flex flex-col gap-2 text-sm">
                {status.data.recent_access.map((a, i) => (
                  <li key={i} className="flex items-center justify-between gap-2">
                    <span className="truncate">
                      <span className="capitalize">{a.action.replace("_", " ")}</span>{" "}
                      <span className="text-muted-foreground">{a.site}</span>
                    </span>
                    <span className="shrink-0 text-xs text-muted-foreground">{timeAgo(a.at)}</span>
                  </li>
                ))}
              </ul>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}

function CredentialRow({
  credential: c,
  onEdit,
  onChanged,
}: {
  credential: Credential;
  onEdit: () => void;
  onChanged: () => void;
}) {
  const [shown, setShown] = useState<string | null>(null);

  // Hide the password again after a while.
  useEffect(() => {
    if (shown === null) return;
    const id = setTimeout(() => setShown(null), REVEAL_MS);
    return () => clearTimeout(id);
  }, [shown]);

  async function reveal() {
    if (shown !== null) {
      setShown(null);
      return;
    }
    try {
      setShown((await revealPassword(c.id)).password);
      onChanged();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  async function copy() {
    try {
      const password = shown ?? (await revealPassword(c.id)).password;
      await navigator.clipboard.writeText(password);
      toast.success("Password copied");
      onChanged();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  async function remove() {
    try {
      await deleteCredential(c.id);
      toast.success(`Deleted the login for ${c.site}`);
      onChanged();
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  return (
    <li className="rounded-xl border bg-card p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="flex items-center gap-2 font-medium">
            {c.site}
            {c.created_by === "agent" ? <Badge tone="primary">Created by the agent</Badge> : null}
          </p>
          <p className="truncate text-sm text-muted-foreground">{c.username}</p>
        </div>
        <div className="flex gap-1">
          <Button variant="outline" size="sm" onClick={reveal} aria-pressed={shown !== null}>
            {shown !== null ? <EyeOff /> : <Eye />} {shown !== null ? "Hide" : "Reveal"}
          </Button>
          <Button variant="outline" size="sm" onClick={copy}>
            <Copy /> Copy
          </Button>
          <Button variant="ghost" size="icon" aria-label={`Edit ${c.site}`} onClick={onEdit}>
            <Pencil />
          </Button>
          <ConfirmDelete onConfirm={remove} />
        </div>
      </div>
      <p className="mt-3 rounded-md bg-muted px-3 py-2 font-mono text-sm" aria-live="polite">
        {shown ?? "••••••••••••"}
      </p>
      <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
        {c.login_url ? (
          <a href={c.login_url} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-1 hover:text-primary">
            <ExternalLink className="size-3" aria-hidden /> Login page
          </a>
        ) : null}
        {c.notes ? <span className="truncate">{c.notes}</span> : null}
        <span className="ml-auto">
          {c.last_used_at ? `Used by the agent ${timeAgo(c.last_used_at)}` : `Added ${timeAgo(c.created_at)}`}
        </span>
      </div>
    </li>
  );
}

function CredentialForm({
  initial,
  onSave,
  onCancel,
}: {
  initial?: Credential;
  onSave: (input: CredentialInput) => Promise<void>;
  onCancel: () => void;
}) {
  const [form, setForm] = useState<CredentialInput>({
    site: initial?.site ?? "",
    login_url: initial?.login_url ?? "",
    username: initial?.username ?? "",
    password: "",
    notes: initial?.notes ?? "",
  });
  const [visible, setVisible] = useState(false);
  const [saving, setSaving] = useState(false);
  const set = (key: keyof CredentialInput, value: string) => setForm((f) => ({ ...f, [key]: value }));

  async function generate() {
    try {
      set("password", (await generatePassword(20)).password);
      setVisible(true);
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    try {
      await onSave(form);
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{initial ? `Edit ${initial.site}` : "New login"}</CardTitle>
        <CardDescription>
          {initial ? "Leave the password empty to keep the current one." : "One login per site."}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form onSubmit={submit} className="grid gap-3 sm:grid-cols-2" autoComplete="off">
          <Input aria-label="Site" placeholder="Site, e.g. seek.com.au" value={form.site} onChange={(e) => set("site", e.target.value)} required autoFocus />
          <Input aria-label="Login page URL" type="url" placeholder="Login page URL (optional)" value={form.login_url} onChange={(e) => set("login_url", e.target.value)} />
          <Input aria-label="Username or email" placeholder="Username or email" value={form.username} onChange={(e) => set("username", e.target.value)} required />
          <div className="flex gap-2">
            <div className="relative flex-1">
              <Input
                aria-label="Password"
                type={visible ? "text" : "password"}
                placeholder={initial ? "New password (optional)" : "Password"}
                value={form.password}
                onChange={(e) => set("password", e.target.value)}
                required={!initial}
                autoComplete="new-password"
                className="pr-9 font-mono"
              />
              <button
                type="button"
                onClick={() => setVisible((v) => !v)}
                aria-label={visible ? "Hide password" : "Show password"}
                className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 text-muted-foreground hover:text-foreground"
              >
                {visible ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
              </button>
            </div>
            <Button variant="outline" onClick={generate} aria-label="Generate a strong password">
              <Wand2 />
            </Button>
          </div>
          <Input aria-label="Notes" placeholder="Notes (optional)" value={form.notes} onChange={(e) => set("notes", e.target.value)} className="sm:col-span-2" />
          <div className="flex justify-end gap-2 sm:col-span-2">
            <Button variant="ghost" onClick={onCancel}>
              Cancel
            </Button>
            <Button type="submit" disabled={saving}>
              Save
            </Button>
          </div>
        </form>
      </CardContent>
    </Card>
  );
}
