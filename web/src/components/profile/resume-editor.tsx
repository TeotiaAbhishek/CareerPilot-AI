"use client";

import { History, Loader2, RotateCcw, Save } from "lucide-react";
import { useEffect, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { toast } from "sonner";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Textarea } from "@/components/ui/textarea";
import { useApi } from "@/hooks/use-api";
import { type ResumeView, errorMessage, getResumeVersion, saveResume } from "@/lib/api";
import { formatDateTime } from "@/lib/format";
import { cn } from "@/lib/utils";

type Mode = "edit" | "preview";

/**
 * Your master resume in Markdown, with a live preview. Every save is kept as a
 * version, and each job score records which version it used.
 */
export function ResumeEditor() {
  const { data, error, mutate } = useApi<ResumeView>("/api/profile/resume");
  const [draft, setDraft] = useState<string | null>(null);
  const [mode, setMode] = useState<Mode>("edit");
  const [saving, setSaving] = useState(false);

  const saved = data?.current?.content ?? "";
  const text = draft ?? saved;
  const dirty = draft !== null && draft.trim() !== saved;

  // Warn before closing the tab with unsaved changes.
  useEffect(() => {
    if (!dirty) return;
    const warn = (event: BeforeUnloadEvent) => event.preventDefault();
    window.addEventListener("beforeunload", warn);
    return () => window.removeEventListener("beforeunload", warn);
  }, [dirty]);

  async function save() {
    setSaving(true);
    try {
      const view = await saveResume(text);
      mutate(() => view);
      setDraft(null);
      toast.success("Resume saved. New scores will use this version.");
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setSaving(false);
    }
  }

  async function loadVersion(id: string) {
    try {
      const version = await getResumeVersion(id);
      setDraft(version.content);
      setMode("edit");
      toast.info(`Loaded the version from ${formatDateTime(version.created_at)}. Save to make it current.`);
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  if (error) return <p className="text-sm text-danger">{error}</p>;
  if (!data) return <Skeleton className="h-96 w-full" />;

  return (
    <div className="grid gap-6 xl:grid-cols-[minmax(0,1fr)_18rem]">
      <Card>
        <CardHeader className="flex-row flex-wrap items-center justify-between gap-3">
          <div>
            <CardTitle>Master resume</CardTitle>
            <CardDescription>
              Markdown. The scoring model reads this, and tailored resumes are built from it.
            </CardDescription>
          </div>
          <div className="flex items-center gap-2">
            <div role="tablist" aria-label="View" className="inline-flex rounded-md border bg-muted p-0.5 xl:hidden">
              {(["edit", "preview"] as const).map((m) => (
                <button
                  key={m}
                  type="button"
                  role="tab"
                  aria-selected={mode === m}
                  onClick={() => setMode(m)}
                  className={cn(
                    "rounded px-3 py-1 text-sm font-medium capitalize",
                    mode === m ? "bg-card shadow-xs" : "text-muted-foreground",
                  )}
                >
                  {m}
                </button>
              ))}
            </div>
            <Button onClick={save} disabled={!dirty || saving || !text.trim()}>
              {saving ? <Loader2 className="animate-spin" /> : <Save />} Save
            </Button>
          </div>
        </CardHeader>
        <CardContent>
          {!data.current && draft === null ? (
            <p className="mb-3 rounded-lg bg-warning/10 p-3 text-sm text-warning">
              No resume yet. Paste it below: scoring waits until there is one.
            </p>
          ) : null}
          <div className="grid gap-4 xl:grid-cols-2">
            <Textarea
              aria-label="Resume (Markdown)"
              value={text}
              onChange={(e) => setDraft(e.target.value)}
              spellCheck
              className={cn("min-h-[32rem] resize-y font-mono text-[13px] leading-relaxed", mode === "preview" && "hidden xl:block")}
            />
            <div
              className={cn(
                "prose prose-sm max-h-[40rem] min-h-[32rem] max-w-none overflow-y-auto rounded-md border bg-background p-4 dark:prose-invert",
                mode === "edit" && "hidden xl:block",
              )}
              aria-label="Preview"
            >
              <ReactMarkdown remarkPlugins={[remarkGfm]}>{text || "*Nothing to preview yet.*"}</ReactMarkdown>
            </div>
          </div>
          <p className="mt-2 flex items-center gap-2 text-xs text-muted-foreground">
            <span className="tabular-nums">{text.length.toLocaleString()} characters</span>
            {dirty ? <Badge tone="warning">Unsaved changes</Badge> : null}
            {dirty ? (
              <button type="button" className="underline-offset-4 hover:underline" onClick={() => setDraft(null)}>
                Discard
              </button>
            ) : null}
          </p>
        </CardContent>
      </Card>

      <Card className="h-fit">
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <History className="size-4" aria-hidden /> Versions
          </CardTitle>
          <CardDescription>Every save is kept. Load one to bring it back.</CardDescription>
        </CardHeader>
        <CardContent>
          {data.versions.length === 0 ? (
            <p className="text-sm text-muted-foreground">None yet.</p>
          ) : (
            <ul className="flex flex-col divide-y">
              {data.versions.map((v, i) => (
                <li key={v.id} className="flex items-center justify-between gap-2 py-2 text-sm">
                  <div>
                    <p className="font-medium">{formatDateTime(v.created_at)}</p>
                    <p className="text-xs text-muted-foreground tabular-nums">
                      {v.chars.toLocaleString()} chars{i === 0 ? " · current" : ""}
                    </p>
                  </div>
                  {i > 0 ? (
                    <Button variant="ghost" size="sm" onClick={() => loadVersion(v.id)} aria-label={`Load version from ${formatDateTime(v.created_at)}`}>
                      <RotateCcw /> Load
                    </Button>
                  ) : null}
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
