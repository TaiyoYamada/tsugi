/** `just now`, `3m ago`, `2h ago`, `5d ago`. `timestamp` is Unix seconds. */
export function ago(timestamp: number, now: number = Date.now() / 1000): string {
  const seconds = Math.max(0, Math.floor(now - timestamp));
  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
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
