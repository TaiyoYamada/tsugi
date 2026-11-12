// Splits the unified diff git prints into files and lines, for display.

export const LINE_KINDS = ["add", "delete", "context", "hunk", "meta"] as const;
export type LineKind = (typeof LINE_KINDS)[number];

export type PatchLine = {
  kind: LineKind;
  text: string;
  /** Line number in the old and in the new file, where the line exists. */
  oldNumber: number | null;
  newNumber: number | null;
};

export type PatchFile = {
  path: string;
  lines: PatchLine[];
  additions: number;
  deletions: number;
};

const FILE_HEADER = /^diff --git a\/(.*) b\/(.*)$/;
const NOTABLE_HEADER = /^(new file mode|deleted file mode|Binary files) /;
const HUNK_HEADER = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/;

export function parsePatch(patch: string): PatchFile[] {
  const files: PatchFile[] = [];
  let file: PatchFile | null = null;
  let oldNumber = 0;
  let newNumber = 0;
  // Before the first hunk of a file, lines such as `--- a/x` are headers,
  // not deletions.
  let inHunk = false;

  for (const text of patch.split("\n")) {
    const header = FILE_HEADER.exec(text);
    if (header) {
      file = { path: header[2] ?? "", lines: [], additions: 0, deletions: 0 };
      files.push(file);
      inHunk = false;
      continue;
    }
    if (!file) continue;

    const hunk = HUNK_HEADER.exec(text);
    if (hunk) {
      oldNumber = Number(hunk[1]);
      newNumber = Number(hunk[2]);
      inHunk = true;
      file.lines.push({ kind: "hunk", text, oldNumber: null, newNumber: null });
    } else if (!inHunk) {
      // Of the header, only what says more than the path is kept: `new file
      // mode`, `deleted file mode`, `Binary files ... differ`.
      if (NOTABLE_HEADER.test(text)) {
        file.lines.push({ kind: "meta", text, oldNumber: null, newNumber: null });
      }
    } else if (text.startsWith("\\")) {
      // `\ No newline at end of file` belongs to the line before it.
      file.lines.push({ kind: "meta", text, oldNumber: null, newNumber: null });
    } else if (text.startsWith("+")) {
      file.lines.push({ kind: "add", text: text.slice(1), oldNumber: null, newNumber });
      file.additions += 1;
      newNumber += 1;
    } else if (text.startsWith("-")) {
      file.lines.push({ kind: "delete", text: text.slice(1), oldNumber, newNumber: null });
      file.deletions += 1;
      oldNumber += 1;
    } else {
      file.lines.push({ kind: "context", text: text.slice(1), oldNumber, newNumber });
      oldNumber += 1;
      newNumber += 1;
    }
  }
  return files;
}
