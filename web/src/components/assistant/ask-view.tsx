"use client";

import { Bot, Loader2, MessageSquarePlus, RotateCcw, Send, TriangleAlert } from "lucide-react";
import Link from "next/link";
import { useRouter, useSearchParams } from "next/navigation";
import { type KeyboardEvent, useEffect, useRef, useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Textarea } from "@/components/ui/textarea";
import { useApi } from "@/hooks/use-api";
import { type Chat, type ChatView, ask, errorMessage } from "@/lib/api";
import { EMPTY_LIVE, type Live, STARTERS, applyEvent, mergeMessages, questionBefore } from "@/lib/assistant";
import { cn } from "@/lib/utils";

import { AnswerText, Chips } from "./answer";
import { ChatList } from "./chat-list";

/** The Ask page: a chat with the assistant, and your past conversations. */
export function AskView() {
  const router = useRouter();
  const chatId = useSearchParams().get("chat");
  const saved = useApi<ChatView>(chatId ? `/api/assistant/chats/${chatId}` : null);
  const chats = useApi<Chat[]>("/api/assistant/chats");
  const [live, setLive] = useState<Live>(EMPTY_LIVE);
  const [question, setQuestion] = useState("");
  const [busy, setBusy] = useState(false);
  const scroller = useRef<HTMLDivElement>(null);

  // What this page added only counts for the chat it was added to.
  const ours = live.chatId === chatId ? live : EMPTY_LIVE;
  const messages = mergeMessages(chatId ? (saved.data?.messages ?? []) : [], ours.messages);
  const draft = ours.draft;
  const empty = !chatId && messages.length === 0 && !draft;

  useEffect(() => {
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages.length, draft?.text, draft?.chips.length]);

  async function send(text: string) {
    const q = text.trim();
    if (!q || busy) return;
    setBusy(true);
    setQuestion("");
    const startedIn = chatId;
    setLive(startedIn === live.chatId ? live : { ...EMPTY_LIVE, chatId: startedIn });
    try {
      await ask(q, startedIn, (event) => {
        setLive((current) => applyEvent(current, event));
        if (event.type === "start" && !startedIn) {
          router.replace(`/ask?chat=${event.chat.id}`, { scroll: false });
          chats.reload();
        }
      });
      chats.reload();
    } catch (e) {
      setQuestion(q);
      toast.error(errorMessage(e));
      setLive((current) => ({ ...current, draft: null }));
    } finally {
      setBusy(false);
    }
  }

  function onKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      send(question);
    }
  }

  return (
    <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_17rem]">
      <Card className="flex h-[calc(100dvh-10rem)] min-h-[26rem] flex-col lg:h-[calc(100dvh-12rem)]">
        <div ref={scroller} className="flex-1 overflow-y-auto p-4 sm:p-6" aria-live="polite">
          {chatId && saved.error ? (
            <p className="text-sm text-danger">{saved.error}</p>
          ) : chatId && !saved.data && ours.messages.length === 0 ? (
            <div className="flex flex-col gap-4">
              <Skeleton className="ml-auto h-10 w-2/3" />
              <Skeleton className="h-24 w-5/6" />
            </div>
          ) : empty ? (
            <div className="flex h-full flex-col items-center justify-center gap-6 py-8 text-center">
              <div className="flex size-12 items-center justify-center rounded-full bg-primary/10 text-primary">
                <Bot className="size-6" aria-hidden />
              </div>
              <div>
                <h2 className="text-base font-semibold">Ask about your jobs and applications</h2>
                <p className="mt-1 max-w-md text-sm text-muted-foreground">
                  The assistant reads your jobs, scores, board and resume on your own computer. It suggests; you
                  decide and act.
                </p>
              </div>
              <div className="grid w-full max-w-xl gap-2 sm:grid-cols-2">
                {STARTERS.map((s) => (
                  <Button
                    key={s}
                    variant="outline"
                    className="h-auto justify-start whitespace-normal px-3 py-2 text-left"
                    disabled={busy}
                    onClick={() => send(s)}
                  >
                    {s}
                  </Button>
                ))}
              </div>
            </div>
          ) : (
            <ol className="flex flex-col gap-5">
              {messages.map((m, i) =>
                m.role === "user" ? (
                  <li key={m.id} className="ml-auto max-w-[85%] rounded-2xl rounded-br-sm bg-primary px-4 py-2 text-sm text-primary-foreground whitespace-pre-wrap">
                    {m.content}
                  </li>
                ) : (
                  <li key={m.id} className="max-w-full">
                    <Chips chips={m.tools} />
                    {m.failed ? (
                      <div className="flex flex-wrap items-start gap-2 rounded-lg border border-danger/30 bg-danger/5 p-3 text-sm text-danger">
                        <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden />
                        <span className="min-w-0 flex-1">{m.content}</span>
                        {i === messages.length - 1 && questionBefore(messages, i) ? (
                          <Button
                            size="sm"
                            variant="outline"
                            disabled={busy}
                            onClick={() => send(questionBefore(messages, i)!)}
                          >
                            <RotateCcw /> Try again
                          </Button>
                        ) : null}
                      </div>
                    ) : (
                      <AnswerText text={m.content} />
                    )}
                  </li>
                ),
              )}
              {draft ? (
                <li className="max-w-full">
                  <Chips chips={draft.chips} />
                  {draft.text ? (
                    <AnswerText text={draft.text} />
                  ) : (
                    <p className="flex items-center gap-2 text-sm text-muted-foreground">
                      <Loader2 className="size-4 animate-spin" aria-hidden />
                      {draft.chips.length ? "Reading what it found…" : "Thinking…"}
                    </p>
                  )}
                </li>
              ) : null}
            </ol>
          )}
        </div>
        <form
          className="flex items-end gap-2 border-t p-3 sm:p-4"
          onSubmit={(e) => {
            e.preventDefault();
            send(question);
          }}
        >
          <Textarea
            aria-label="Your question"
            placeholder="Ask about your jobs, applications or resume…"
            rows={2}
            maxLength={2000}
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
            onKeyDown={onKeyDown}
            className="max-h-40 min-h-[2.75rem] resize-none"
          />
          <Button type="submit" size="icon" disabled={busy || !question.trim()} aria-label="Send">
            {busy ? <Loader2 className="animate-spin" /> : <Send />}
          </Button>
        </form>
      </Card>

      <Card className="h-fit">
        <CardContent className="flex flex-col gap-3 pt-5">
          <div className="flex items-center justify-between gap-2">
            <h2 className="text-sm font-semibold">Conversations</h2>
            <Link
              href="/ask"
              className={cn("inline-flex items-center gap-1 text-sm font-medium text-primary hover:underline", !chatId && "pointer-events-none opacity-50")}
              aria-disabled={!chatId}
            >
              <MessageSquarePlus className="size-4" aria-hidden /> New
            </Link>
          </div>
          <ChatList
            chats={chats.data}
            error={chats.error}
            activeId={chatId}
            onChanged={() => {
              chats.reload();
              saved.reload();
            }}
            onDeleted={(id) => {
              chats.reload();
              if (id === chatId) router.replace("/ask");
            }}
          />
        </CardContent>
      </Card>
    </div>
  );
}
