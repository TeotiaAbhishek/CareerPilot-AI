"use client";

import { Plus, Search as SearchIcon, Trash2 } from "lucide-react";
import { type FormEvent, useState } from "react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import { useApi } from "@/hooks/use-api";
import { type Search, createSearch, deleteSearch, errorMessage, updateSearch } from "@/lib/api";

/** The SEEK searches the scraper runs: list, add, switch on/off, delete. */
export function SearchesManager() {
  const { data: searches, error, mutate } = useApi<Search[]>("/api/searches");

  async function toggle(search: Search, enabled: boolean) {
    try {
      const updated = await updateSearch(search.id, { ...search, enabled });
      mutate((all) => all.map((s) => (s.id === updated.id ? updated : s)));
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  async function remove(search: Search) {
    try {
      await deleteSearch(search.id);
      mutate((all) => all.filter((s) => s.id !== search.id));
      toast.success(`Deleted “${search.keywords}”`);
    } catch (e) {
      toast.error(errorMessage(e));
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>SEEK searches</CardTitle>
        <CardDescription>Every enabled search runs on each scrape, newest jobs first.</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        {error ? (
          <p className="text-sm text-danger">{error}</p>
        ) : !searches ? (
          <Skeleton className="h-24 w-full" />
        ) : searches.length === 0 ? (
          <p className="text-sm text-muted-foreground">No searches yet. Add one below.</p>
        ) : (
          <ul className="divide-y rounded-lg border">
            {searches.map((search) => (
              <SearchRow key={search.id} search={search} onToggle={toggle} onDelete={remove} />
            ))}
          </ul>
        )}

        <AddSearchForm onAdded={(s) => mutate((all) => [...all, s])} />
      </CardContent>
    </Card>
  );
}

function SearchRow({
  search,
  onToggle,
  onDelete,
}: {
  search: Search;
  onToggle: (s: Search, enabled: boolean) => void;
  onDelete: (s: Search) => void;
}) {
  // First click arms the button, second click deletes. No modal needed.
  const [confirming, setConfirming] = useState(false);

  return (
    <li className="flex items-center gap-4 px-4 py-3">
      <SearchIcon className="size-4 shrink-0 text-muted-foreground" aria-hidden />
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium">{search.keywords}</p>
        <p className="text-xs text-muted-foreground">
          {search.location} · {search.max_pages} {search.max_pages === 1 ? "page" : "pages"}
        </p>
      </div>
      <Switch
        checked={search.enabled}
        onCheckedChange={(enabled) => onToggle(search, enabled)}
        label={`Enable ${search.keywords}`}
      />
      <Button
        variant={confirming ? "danger" : "ghost"}
        size={confirming ? "sm" : "icon"}
        aria-label={`Delete ${search.keywords}`}
        onClick={() => (confirming ? onDelete(search) : setConfirming(true))}
        onBlur={() => setConfirming(false)}
      >
        <Trash2 />
        {confirming ? "Delete?" : null}
      </Button>
    </li>
  );
}

function AddSearchForm({ onAdded }: { onAdded: (s: Search) => void }) {
  const [keywords, setKeywords] = useState("");
  const [location, setLocation] = useState("Sydney NSW");
  const [pages, setPages] = useState("3");
  const [saving, setSaving] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    try {
      const created = await createSearch({
        keywords,
        location,
        max_pages: Number(pages),
        enabled: true,
      });
      onAdded(created);
      setKeywords("");
      toast.success(`Added “${created.keywords}”`);
    } catch (e) {
      toast.error(errorMessage(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <form onSubmit={submit} className="grid gap-2 sm:grid-cols-[2fr_1.5fr_auto_auto]">
      <Input
        value={keywords}
        onChange={(e) => setKeywords(e.target.value)}
        placeholder="Keywords, e.g. BI Developer"
        aria-label="Keywords"
        required
      />
      <Input
        value={location}
        onChange={(e) => setLocation(e.target.value)}
        placeholder="Location"
        aria-label="Location"
        required
      />
      <Select value={pages} onChange={(e) => setPages(e.target.value)} aria-label="Pages" className="w-28">
        {[1, 2, 3, 5, 10].map((n) => (
          <option key={n} value={n}>
            {n} {n === 1 ? "page" : "pages"}
          </option>
        ))}
      </Select>
      <Button type="submit" disabled={saving || !keywords.trim()}>
        <Plus /> Add
      </Button>
    </form>
  );
}
