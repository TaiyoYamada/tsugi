import { expect, test } from "vitest";

import { parsePatch } from "./patch";

const PATCH = `diff --git a/src/auth.rs b/src/auth.rs
index 1111111..2222222 100644
--- a/src/auth.rs
+++ b/src/auth.rs
@@ -10,4 +10,5 @@ fn login() {
 context
-old line
+new line
+another
 tail
\\ No newline at end of file
diff --git a/notes.txt b/notes.txt
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/notes.txt
@@ -0,0 +1 @@
+--- not a header`;

test("splits a patch into files with line numbers", () => {
  const files = parsePatch(PATCH);
  expect(files.map((file) => file.path)).toEqual(["src/auth.rs", "notes.txt"]);

  const [auth, notes] = files;
  expect(auth?.additions).toBe(2);
  expect(auth?.deletions).toBe(1);
  expect(
    auth?.lines
      .filter((line) => line.kind !== "meta" && line.kind !== "hunk")
      .map((line) => [line.kind, line.oldNumber, line.newNumber, line.text]),
  ).toEqual([
    ["context", 10, 10, "context"],
    ["delete", 11, null, "old line"],
    ["add", null, 11, "new line"],
    ["add", null, 12, "another"],
    ["context", 12, 13, "tail"],
  ]);

  // `index`, `---` and `+++` repeat the path and are dropped.
  expect(auth?.lines.filter((line) => line.kind === "meta")).toEqual([
    {
      kind: "meta",
      text: "\\ No newline at end of file",
      oldNumber: null,
      newNumber: null,
    },
  ]);
  expect(notes?.lines[0]?.text).toBe("new file mode 100644");

  // Inside a hunk, a line that starts with `---` is content.
  expect(notes?.additions).toBe(1);
  expect(notes?.lines.at(-1)).toEqual({
    kind: "add",
    text: "--- not a header",
    oldNumber: null,
    newNumber: 1,
  });
});

test("an empty patch has no files", () => {
  expect(parsePatch("")).toEqual([]);
});
