"use client";

import { MessageSquarePlus } from "lucide-react";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";

import { Card, CardContent } from "@/components/ui/card";
import { useApi } from "@/hooks/use-api";
import type { Chat } from "@/lib/api";
import { cn } from "@/lib/utils";

import { ChatList } from "./chat-list";
import { ChatPanel } from "./chat-panel";

/** The Ask page: a chat with the assistant, and your past conversations. */
export function AskView() {
  const router = useRouter();
  const chatId = useSearchParams().get("chat");
  const chats = useApi<Chat[]>("/api/assistant/chats");
  const { reload } = chats;

  return (
    <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_17rem]">
      <Card className="overflow-hidden">
        <ChatPanel
          chatId={chatId}
          onStarted={(chat) => {
            router.replace(`/ask?chat=${chat.id}`, { scroll: false });
            reload();
          }}
          onAnswered={reload}
          className="h-[calc(100dvh-10rem)] min-h-[26rem] lg:h-[calc(100dvh-12rem)]"
        />
      </Card>

      <Card className="h-fit">
        <CardContent className="flex flex-col gap-3 pt-5">
          <div className="flex items-center justify-between gap-2">
            <h2 className="text-sm font-semibold">Conversations</h2>
            <Link
              href="/ask"
              className={cn(
                "inline-flex items-center gap-1 text-sm font-medium text-primary hover:underline",
                !chatId && "pointer-events-none opacity-50",
              )}
              aria-disabled={!chatId}
            >
              <MessageSquarePlus className="size-4" aria-hidden /> New
            </Link>
          </div>
          <ChatList
            chats={chats.data}
            error={chats.error}
            activeId={chatId}
            onChanged={reload}
            onDeleted={(id) => {
              reload();
              if (id === chatId) router.replace("/ask");
            }}
          />
        </CardContent>
      </Card>
    </div>
  );
}
