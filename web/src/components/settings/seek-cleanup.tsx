"use client";

import { FileText, Loader2, Search, Trash2 } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { type SeekScan, deleteSeekResume, errorMessage, scanSeekResumes } from "@/lib/api";

/**
 * SEEK keeps every resume you upload. This lists the ones CareerPilot uploaded
 * ("Your Name - Resume.pdf"), ticks all but the newest 3, and deletes the ones
 * you leave ticked. Resumes you uploaded yourself are never listed.
 */
export function SeekCleanup() {
  const [scan, setScan] = useState<SeekScan | null>(null);
  const [picked, setPicked] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState<"scan" | "delete" | null>(null);

  async function find(practice: boolean) {
    setBusy("scan");
    try {
      const found = await scanSeekResumes(practice);
      setScan(found);
      setPicked(new Set(found.resumes.filter((r) => r.delete).map((r) => r.index)));
      if (found.resumes.length === 0) toast.info("No CareerPilot resumes on that page.");
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(null);
    }
  }

  async function remove() {
    if (!scan) return;
    setBusy("delete");
    const deleted: string[] = [];
    const failed: string[] = [];
    // One request each, highest position first, so the others don't move.
    const chosen = scan.resumes.filter((r) => picked.has(r.index)).sort((a, b) => b.index - a.index);
    try {
      for (const r of chosen) {
        const result = await deleteSeekResume(scan.tab, { index: r.index, name: r.name });
        deleted.push(...result.deleted);
        failed.push(...result.failed);
      }
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setBusy(null);
    }
    if (deleted.length) toast.success(`Deleted ${deleted.length} old resume${deleted.length === 1 ? "" : "s"} on SEEK.`);
    if (failed.length) toast.error(`Couldn't delete ${failed.length} safely. Delete them yourself on SEEK.`);
    setScan(null);
    setPicked(new Set());
  }

  function toggle(index: number) {
    setPicked((old) => {
      const next = new Set(old);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Old resumes on SEEK</CardTitle>
        <CardDescription>
          Every application uploads a fresh resume, and SEEK keeps them all. This lists only the ones CareerPilot
          uploaded, ticks all but the newest 3, and deletes the ones you leave ticked.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4 text-sm">
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" disabled={busy !== null} onClick={() => find(false)}>
            {busy === "scan" ? <Loader2 className="animate-spin" /> : <Search />} Find old resumes on SEEK
          </Button>
          <Button variant="ghost" disabled={busy !== null} onClick={() => find(true)}>
            Try it on the practice page
          </Button>
        </div>

        {scan && scan.resumes.length > 0 ? (
          <>
            <ul className="flex flex-col divide-y rounded-lg border">
              {scan.resumes.map((r) => (
                <li key={r.index}>
                  <label className="flex cursor-pointer items-center gap-3 px-3 py-2">
                    <input
                      type="checkbox"
                      className="size-4 accent-primary"
                      checked={picked.has(r.index)}
                      disabled={busy !== null}
                      onChange={() => toggle(r.index)}
                    />
                    <FileText className="size-4 text-muted-foreground" aria-hidden />
                    <span className="flex-1">{r.name}</span>
                    <span className="text-xs text-muted-foreground">{r.added || "date unknown"}</span>
                  </label>
                </li>
              ))}
            </ul>
            <div>
              <Button variant="danger" disabled={busy !== null || picked.size === 0} onClick={remove}>
                {busy === "delete" ? <Loader2 className="animate-spin" /> : <Trash2 />} Delete {picked.size} ticked
              </Button>
            </div>
          </>
        ) : null}
      </CardContent>
    </Card>
  );
}
