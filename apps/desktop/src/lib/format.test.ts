import { expect, test } from "vitest";

import { age, baseName, shortBranch } from "./format";

test("age", () => {
  expect(age(1000, 1030)).toEqual({ unit: "now" });
  expect(age(1000, 1000 + 5 * 60)).toEqual({ unit: "minutes", count: 5 });
  expect(age(1000, 1000 + 3 * 3600)).toEqual({ unit: "hours", count: 3 });
  expect(age(1000, 1000 + 2 * 86400)).toEqual({ unit: "days", count: 2 });
  // A clock that runs behind must not produce a negative age.
  expect(age(1000, 900)).toEqual({ unit: "now" });
});

test("names", () => {
  expect(shortBranch("refs/heads/tsugi/login")).toBe("tsugi/login");
  expect(shortBranch("main")).toBe("main");
  expect(baseName("/work/my-repo/")).toBe("my-repo");
});
