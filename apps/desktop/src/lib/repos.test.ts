import { expect, test } from "vitest";

import { parseRepoList, withRepo, withoutRepo } from "./repos";

test("the stored list is read, and the shown repository must be on it", () => {
  expect(parseRepoList('["/a","/b"]', "/b")).toEqual({
    repos: ["/a", "/b"],
    current: "/b",
  });
  expect(parseRepoList('["/a","/b"]', "/gone")).toEqual({
    repos: ["/a", "/b"],
    current: "/a",
  });
  expect(parseRepoList("[]", "/a")).toEqual({ repos: [], current: null });
  expect(parseRepoList(null, null)).toEqual({ repos: [], current: null });
  expect(parseRepoList("not json", null)).toEqual({ repos: [], current: null });
  expect(parseRepoList('["/a", 5, null]', null)).toEqual({
    repos: ["/a"],
    current: "/a",
  });
});

test("a single repository stored by an older version becomes the list", () => {
  expect(parseRepoList(null, "/a")).toEqual({ repos: ["/a"], current: "/a" });
});

test("adding shows the repository and does not list it twice", () => {
  const list = { repos: ["/a"], current: "/a" };
  expect(withRepo(list, "/b")).toEqual({ repos: ["/a", "/b"], current: "/b" });
  expect(withRepo({ repos: ["/a", "/b"], current: "/b" }, "/a")).toEqual({
    repos: ["/a", "/b"],
    current: "/a",
  });
});

test("removing the shown repository shows its neighbour", () => {
  const repos = ["/a", "/b", "/c"];
  expect(withoutRepo({ repos, current: "/b" }, "/b")).toEqual({
    repos: ["/a", "/c"],
    current: "/c",
  });
  expect(withoutRepo({ repos, current: "/c" }, "/c")).toEqual({
    repos: ["/a", "/b"],
    current: "/b",
  });
  expect(withoutRepo({ repos, current: "/a" }, "/c")).toEqual({
    repos: ["/a", "/b"],
    current: "/a",
  });
  expect(withoutRepo({ repos: ["/a"], current: "/a" }, "/a")).toEqual({
    repos: [],
    current: null,
  });
  expect(withoutRepo({ repos, current: "/a" }, "/x")).toEqual({
    repos,
    current: "/a",
  });
});
