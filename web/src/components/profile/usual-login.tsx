"use client";

import { KeyRound } from "lucide-react";
import { type FormEvent, useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { useApi } from "@/hooks/use-api";
import { type UsualLogin, deleteUsualLogin, errorMessage, saveUsualLogin } from "@/lib/api";

import { ConfirmDelete } from "./answers-manager";

/**
 * The email and password you use on most sites. On a site with no saved
 * login, the agent tries this once before offering to create an account,
 * and saves it for that site if it works.
 */
export function UsualLoginCard() {
  const usual = useApi<UsualLogin>("/api/vault/default");
  const [editing, setEditing] = useState(false);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);

  async function save(event: FormEvent) {
    event.preventDefault();
    setBusy(true);
    try {
      const saved = await saveUsualLogin(username.trim(), password);
      usual.mutate(() => saved);
      setPassword("");
      setEditing(false);
      toast.success("Usual login saved");
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    try {
      await deleteUsualLogin();
      usual.mutate(() => ({ set: false, username: "" }));
      toast.success("Usual login removed");
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  function edit() {
    setUsername(usual.data?.username ?? "");
    setPassword("");
    setEditing(true);
  }

  const u = usual.data;

  return (
    <Card className="h-fit">
      <CardHeader>
        <CardTitle>Usual login</CardTitle>
        <CardDescription>
          On a site with no saved login, the agent tries this once before offering to create an account. If it works,
          it&apos;s saved for that site.
        </CardDescription>
      </CardHeader>
      <CardContent className="text-sm">
        {usual.error ? (
          <p className="text-danger">{usual.error}</p>
        ) : !u ? (
          <Skeleton className="h-10 w-full" />
        ) : editing ? (
          <form onSubmit={save} className="flex flex-col gap-3">
            <label className="flex flex-col gap-1.5">
              <span className="font-medium">Email or username</span>
              <Input
                value={username}
                maxLength={200}
                autoComplete="off"
                placeholder="Leave empty to use your profile email"
                onChange={(e) => setUsername(e.target.value)}
              />
            </label>
            <label className="flex flex-col gap-1.5">
              <span className="font-medium">Password</span>
              <Input
                type="password"
                required
                maxLength={200}
                autoComplete="new-password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
              />
            </label>
            <div className="flex gap-2">
              <Button type="submit" disabled={busy || !password}>
                Save
              </Button>
              <Button type="button" variant="ghost" onClick={() => setEditing(false)}>
                Cancel
              </Button>
            </div>
          </form>
        ) : u.set ? (
          <div className="flex flex-wrap items-center gap-2">
            <KeyRound className="size-4 text-muted-foreground" aria-hidden />
            <span className="mr-auto truncate">{u.username || "Your profile email"} · ••••••••</span>
            <Button size="sm" variant="outline" onClick={edit}>
              Change
            </Button>
            <ConfirmDelete onConfirm={remove} />
          </div>
        ) : (
          <Button variant="outline" onClick={edit}>
            <KeyRound /> Add your usual login
          </Button>
        )}
      </CardContent>
    </Card>
  );
}
