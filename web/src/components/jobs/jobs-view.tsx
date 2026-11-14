"use client";

import { Briefcase, MousePointerClick, SearchX, Sparkles } from "lucide-react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { useCallback, useEffect, useMemo } from "react";

import { EmptyState } from "@/components/ui/empty-state";
import { PageHeader } from "@/components/ui/page-header";
import { useApi } from "@/hooks/use-api";
import { type Facet, type JobDetail, type JobPage, summaryPatch } from "@/lib/api";
import {
  DEFAULT_FILTERS,
  type JobFilters,
  filtersToApiPath,
  filtersToSearchParams,
  parseFilters,
} from "@/lib/jobs-query";

import { JobDetailView } from "./job-detail";
import { JobFiltersBar } from "./job-filters";
import { JobList, JobListSkeleton, Pagination } from "./job-list";
import { ScrapeButton } from "./scrape-button";

const DESKTOP = "(min-width: 1024px)";

/**
 * The Jobs page: filters on top, list on the left, the selected job on the right.
 * Everything you see is described by the URL, e.g. /jobs?q=sql&status=new&job=123.
 */
export function JobsView() {
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();

  const filters = useMemo(() => parseFilters(searchParams), [searchParams]);
  const selectedId = searchParams.get("job");

  const list = useApi<JobPage>(filtersToApiPath(filters));
  const workTypes = useApi<Facet[]>("/api/jobs/work-types");
  const jobs = useMemo(() => list.data?.items ?? [], [list.data]);

  const navigate = useCallback(
    (next: JobFilters, job: string | null) => {
      const query = filtersToSearchParams(next, job);
      router.replace(query ? `${pathname}?${query}` : pathname, { scroll: false });
    },
    [router, pathname],
  );

  const changeFilters = useCallback(
    (patch: Partial<JobFilters>) => navigate({ ...filters, ...patch }, null),
    [filters, navigate],
  );

  const select = useCallback(
    (id: string) => {
      // Small screens have no side pane: open the job's own page instead.
      if (!window.matchMedia(DESKTOP).matches) {
        router.push(`/jobs/${encodeURIComponent(id)}`);
        return;
      }
      navigate(filters, id);
      document.getElementById(`job-${id}`)?.scrollIntoView({ block: "nearest" });
    },
    [filters, navigate, router],
  );

  // Keep the list in step with changes made in the detail pane.
  const { mutate, reload } = list;
  const handleJobChange = useCallback(
    (updated: JobDetail) =>
      mutate((page) => ({
        ...page,
        items: page.items.map((j) => (j.id === updated.id ? { ...j, ...summaryPatch(updated) } : j)),
      })),
    [mutate],
  );

  // ↑/↓ or j/k move through the list (unless you're typing in a field).
  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      const target = event.target as HTMLElement;
      if (target.closest("input, select, textarea") || jobs.length === 0) return;

      const step = event.key === "ArrowDown" || event.key === "j" ? 1 : event.key === "ArrowUp" || event.key === "k" ? -1 : 0;
      if (step === 0) return;
      event.preventDefault();
      const index = jobs.findIndex((j) => j.id === selectedId);
      const next = jobs[Math.min(jobs.length - 1, Math.max(0, index + step))];
      if (next) select(next.id);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [jobs, selectedId, select]);

  const filtered = JSON.stringify(filters) !== JSON.stringify(DEFAULT_FILTERS);

  return (
    <>
      <PageHeader
        title="Jobs"
        description={
          list.data ? `${list.data.total} jobs${filtered ? " match your filters" : ""}` : "Loading…"
        }
        actions={<ScrapeButton onFinished={reload} />}
      />

      <JobFiltersBar filters={filters} workTypes={workTypes.data ?? []} onChange={changeFilters} />

      <div className="mt-6 grid gap-6 lg:grid-cols-[minmax(0,2fr)_minmax(0,3fr)]">
        <section aria-label="Job list">
          {list.error ? (
            <EmptyState icon={SearchX} title="Couldn't load jobs" description={list.error} />
          ) : !list.data ? (
            <JobListSkeleton />
          ) : jobs.length === 0 ? (
            filters.relevantOnly && list.data.total === 0 && !filtered ? (
              <EmptyState
                icon={Sparkles}
                title="No relevant jobs yet"
                description="Jobs show up here once your local model scores them at or above your threshold. Switch off “Relevant only” to see every job."
              />
            ) : filtered ? (
              <EmptyState
                icon={SearchX}
                title="No jobs match"
                description="Try a different search or clear some filters."
              />
            ) : (
              <EmptyState
                icon={Briefcase}
                title="No jobs yet"
                description="Click “Scrape now” to search SEEK, or import your v1 jobs (see the F2 guide)."
              />
            )
          ) : (
            <div className={list.loading ? "opacity-60 transition-opacity" : undefined}>
              <JobList jobs={jobs} selectedId={selectedId} onSelect={select} />
              <Pagination
                page={list.data.page}
                pageSize={list.data.page_size}
                total={list.data.total}
                onPage={(page) => changeFilters({ page })}
              />
            </div>
          )}
        </section>

        <section aria-label="Job details" className="hidden lg:block">
          <div className="sticky top-20">
            {selectedId ? (
              <JobDetailView id={selectedId} onChange={handleJobChange} shortcuts />
            ) : (
              <EmptyState
                icon={MousePointerClick}
                title="Pick a job"
                description="Select a job on the left to read it here. Tips: ↑ and ↓ move through the list; a, m and n label it Apply, Maybe or No."
              />
            )}
          </div>
        </section>
      </div>
    </>
  );
}
