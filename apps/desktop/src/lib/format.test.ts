import { expect, test } from "vitest";

import { ago, baseName, shortBranch } from "./format";

test("ago", () => {
  expect(ago(1000, 1030)).toBe("just now");
  expect(ago(1000, 1000 + 5 * 60)).toBe("5m ago");
  expect(ago(1000, 1000 + 3 * 3600)).toBe("3h ago");
  expect(ago(1000, 1000 + 2 * 86400)).toBe("2d ago");
  // A clock that runs behind must not produce a negative age.
  expect(ago(1000, 900)).toBe("just now");
});

test("names", () => {
  expect(shortBranch("refs/heads/tsugi/login")).toBe("tsugi/login");
  expect(shortBranch("main")).toBe("main");
  expect(baseName("/work/my-repo/")).toBe("my-repo");
});
