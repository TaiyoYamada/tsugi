// The list of repositories that are open in the app, and which of them is
// on screen.

export type RepoList = {
  /** Roots of the open repositories, in the order they were added. */
  repos: string[];
  current: string | null;
};

/** The strings of a stored JSON array; nothing if it is anything else. */
export function storedStrings(stored: string | null): string[] {
  let value: unknown;
  try {
    value = JSON.parse(stored ?? "[]");
  } catch {
    return [];
  }
  return Array.isArray(value)
    ? value.filter((entry): entry is string => typeof entry === "string")
    : [];
}

/**
 * Reads the stored list. Versions of the app that showed one repository
 * stored only `storedCurrent`; that repository becomes the list.
 */
export function parseRepoList(
  storedRepos: string | null,
  storedCurrent: string | null,
): RepoList {
  let repos = storedStrings(storedRepos);
  if (storedRepos === null && storedCurrent !== null) repos = [storedCurrent];
  const current =
    storedCurrent !== null && repos.includes(storedCurrent)
      ? storedCurrent
      : (repos[0] ?? null);
  return { repos, current };
}

/** Adds a repository, unless it is on the list, and shows it. */
export function withRepo(list: RepoList, root: string): RepoList {
  return {
    repos: list.repos.includes(root) ? list.repos : [...list.repos, root],
    current: root,
  };
}

/**
 * Takes a repository off the list. If it was on screen, the one after it
 * takes its place, or the one before it if it was the last.
 */
export function withoutRepo(list: RepoList, root: string): RepoList {
  const index = list.repos.indexOf(root);
  if (index === -1) return list;
  const repos = list.repos.filter((entry) => entry !== root);
  const current =
    list.current === root
      ? (repos[index] ?? repos[index - 1] ?? null)
      : list.current;
  return { repos, current };
}
