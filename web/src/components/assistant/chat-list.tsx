"use client";

import { Check, MessagesSquare, Pencil, Trash2, X } from "lucide-react";
import Link from "next/link";
import { useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { type Chat, deleteChat, errorMessage, renameChat } from "@/lib/api";
import { timeAgo } from "@/lib/format";
import { cn } from "@/lib/utils";

/** Past conversations: open, rename or delete one. */
export function ChatList({
  chats,
  error,
  activeId,
  onChanged,
  onDeleted,
}: {
  chats?: Chat[];
  error?: string;
  activeId: string | null;
  onChanged: () => void;
  onDeleted: (id: string) => void;
}) {
  const [editing, setEditing] = useState<{ id: string; title: string } | null>(null);
  const [confirming, setConfirming] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function act(action: () => Promise<void>) {
    setBusy(true);
    try {
      await action();
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  if (error) return <p className="text-sm text-danger">{error}</p>;
  if (!chats) return <Skeleton className="h-32 w-full" />;
  if (chats.length === 0)
    return (
      <p className="flex items-center gap-2 text-sm text-muted-foreground">
        <MessagesSquare className="size-4" aria-hidden /> Your conversations will be listed here.
      </p>
    );

  return (
    <ul className="flex flex-col gap-1">
      {chats.map((chat) =>
        editing?.id === chat.id ? (
          <li key={chat.id}>
            <form
              className="flex items-center gap-1"
              onSubmit={(e) => {
                e.preventDefault();
                act(async () => {
                  await renameChat(chat.id, editing.title);
                  setEditing(null);
                  onChanged();
                });
              }}
            >
              <Input
                aria-label="Conversation title"
                value={editing.title}
                maxLength={100}
                autoFocus
                onChange={(e) => setEditing({ id: chat.id, title: e.target.value })}
                className="h-8"
              />
              <Button type="submit" size="icon" variant="ghost" disabled={busy} aria-label="Save title" className="size-8">
                <Check />
              </Button>
              <Button size="icon" variant="ghost" aria-label="Cancel" className="size-8" onClick={() => setEditing(null)}>
                <X />
              </Button>
            </form>
          </li>
        ) : (
          <li
            key={chat.id}
            className={cn(
              "group flex items-center gap-1 rounded-md pr-1",
              chat.id === activeId ? "bg-primary/10 text-primary" : "hover:bg-muted",
            )}
          >
            <Link href={`/ask?chat=${chat.id}`} className="min-w-0 flex-1 px-3 py-2">
              <span className="block truncate text-sm font-medium">{chat.title}</span>
              <span className="block text-xs text-muted-foreground">{timeAgo(chat.updated_at)}</span>
            </Link>
            {confirming === chat.id ? (
              <Button
                size="sm"
                variant="danger"
                disabled={busy}
                onClick={() =>
                  act(async () => {
                    await deleteChat(chat.id);
                    setConfirming(null);
                    onDeleted(chat.id);
                  })
                }
              >
                Delete
              </Button>
            ) : null}
            {confirming === chat.id ? (
              <Button size="icon" variant="ghost" aria-label="Keep it" className="size-8" onClick={() => setConfirming(null)}>
                <X />
              </Button>
            ) : (
              <>
                <Button
                  size="icon"
                  variant="ghost"
                  className="size-8 text-muted-foreground"
                  aria-label={`Rename ${chat.title}`}
                  onClick={() => setEditing({ id: chat.id, title: chat.title })}
                >
                  <Pencil />
                </Button>
                <Button
                  size="icon"
                  variant="ghost"
                  className="size-8 text-muted-foreground"
                  aria-label={`Delete ${chat.title}`}
                  onClick={() => setConfirming(chat.id)}
                >
                  <Trash2 />
                </Button>
              </>
            )}
          </li>
        ),
      )}
    </ul>
  );
}
