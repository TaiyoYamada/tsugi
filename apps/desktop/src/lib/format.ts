export type Age =
  | { unit: "now" }
  | { unit: "minutes" | "hours" | "days"; count: number };

/** How long ago `timestamp` (Unix seconds) was, in the largest whole unit. */
export function age(timestamp: number, now: number = Date.now() / 1000): Age {
  const seconds = Math.max(0, Math.floor(now - timestamp));
  if (seconds < 60) return { unit: "now" };
  if (seconds < 3600) return { unit: "minutes", count: Math.floor(seconds / 60) };
  if (seconds < 86400) return { unit: "hours", count: Math.floor(seconds / 3600) };
  return { unit: "days", count: Math.floor(seconds / 86400) };
}

export function shortId(id: string): string {
  return id.slice(0, 8);
}

/** `refs/heads/main` → `main`. */
export function shortBranch(branch: string): string {
  return branch.startsWith("refs/heads/")
    ? branch.slice("refs/heads/".length)
    : branch;
}

/** Last component of a path. */
export function baseName(path: string): string {
  const parts = path.split("/").filter((part) => part !== "");
  return parts[parts.length - 1] ?? path;
}
