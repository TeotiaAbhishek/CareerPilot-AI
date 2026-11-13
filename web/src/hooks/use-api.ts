"use client";

import { useCallback, useEffect, useState } from "react";

import { apiGet, errorMessage } from "@/lib/api";

type Result<T> = { key: string; data?: T; error?: string };

/**
 * Load JSON from the API and re-load whenever `path` changes.
 *  - `loading` is true while the answer for the *current* path is on its way.
 *  - `data` keeps the previous answer meanwhile, so lists don't flash empty.
 *  - `reload()` fetches the same path again; `mutate()` edits the data locally.
 * Pass `null` as the path to load nothing.
 */
export function useApi<T>(path: string | null) {
  const [version, setVersion] = useState(0);
  const [result, setResult] = useState<Result<T> | null>(null);
  const key = path === null ? null : `${version}:${path}`;

  useEffect(() => {
    if (key === null || path === null) return;
    let cancelled = false;

    apiGet<T>(path, { cache: "no-store" }).then(
      (data) => !cancelled && setResult({ key, data }),
      (error) => !cancelled && setResult({ key, error: errorMessage(error) }),
    );

    // If the path changes before the answer arrives, ignore the old answer.
    return () => {
      cancelled = true;
    };
  }, [key, path]);

  const current = result?.key === key ? result : null;

  const reload = useCallback(() => setVersion((v) => v + 1), []);
  const mutate = useCallback(
    (update: (data: T) => T) =>
      setResult((r) => (r?.data === undefined ? r : { ...r, data: update(r.data) })),
    [],
  );

  return {
    data: current ? current.data : result?.data,
    error: current?.error,
    loading: key !== null && current === null,
    reload,
    mutate,
  };
}
