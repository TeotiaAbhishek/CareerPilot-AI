/** "just now", "5 min ago", "3 h ago", "2 days ago", then a date. */
export function timeAgo(iso: string | null | undefined, now: Date = new Date()): string {
  if (!iso) return "";
  const then = new Date(iso);
  const seconds = Math.round((now.getTime() - then.getTime()) / 1000);

  if (seconds < 60) return "just now";
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.round(hours / 24);
  if (days === 1) return "yesterday";
  if (days < 30) return `${days} days ago`;
  return then.toLocaleDateString("en-AU", { day: "numeric", month: "short", year: "numeric" });
}

/** "in 9 h", "in 25 min" for times in the future. */
export function timeUntil(iso: string | null | undefined, now: Date = new Date()): string {
  if (!iso) return "";
  const minutes = Math.round((new Date(iso).getTime() - now.getTime()) / 60_000);
  if (minutes < 1) return "now";
  if (minutes < 60) return `in ${minutes} min`;
  return `in ${Math.round(minutes / 60)} h`;
}

export function formatDateTime(iso: string | null | undefined): string {
  if (!iso) return "";
  return new Date(iso).toLocaleString("en-AU", {
    day: "numeric",
    month: "short",
    hour: "numeric",
    minute: "2-digit",
  });
}

/** "16 Nov", or "16 Nov 2025" outside this year. */
export function formatDate(iso: string | null | undefined, now: Date = new Date()): string {
  if (!iso) return "";
  const date = new Date(iso);
  return date.toLocaleDateString("en-AU", {
    day: "numeric",
    month: "short",
    year: date.getFullYear() === now.getFullYear() ? undefined : "numeric",
  });
}
