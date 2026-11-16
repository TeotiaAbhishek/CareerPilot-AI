"use client";

import { ArrowRight, MessageSquareText } from "lucide-react";
import Link from "next/link";
import { useState } from "react";

import { ChatPanel } from "@/components/assistant/chat-panel";
import { Card, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";

/**
 * Ask from the Dashboard. Every visit starts a new conversation; each one is
 * saved and listed on the Ask page.
 */
export function QuickAsk() {
  const [chatId, setChatId] = useState<string | null>(null);

  return (
    <Card className="flex h-full flex-col overflow-hidden">
      <CardHeader>
        <div className="flex items-start justify-between gap-2">
          <div>
            <CardTitle className="flex items-center gap-2">
              <MessageSquareText className="size-4 text-primary" aria-hidden /> Ask
            </CardTitle>
            <CardDescription>A new conversation each time. Past ones are on the Ask page.</CardDescription>
          </div>
          <Link
            href={chatId ? `/ask?chat=${chatId}` : "/ask"}
            className="inline-flex shrink-0 items-center gap-1 text-sm font-medium text-primary hover:underline"
          >
            {chatId ? "Open in Ask" : "History"} <ArrowRight className="size-4" aria-hidden />
          </Link>
        </div>
      </CardHeader>
      <ChatPanel
        chatId={chatId}
        onStarted={(chat) => setChatId(chat.id)}
        compact
        className="h-[26rem] border-t"
      />
    </Card>
  );
}
