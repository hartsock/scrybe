// SPDX-License-Identifier: Apache-2.0
export interface LineHunk {
  kind: "same" | "removed" | "added" | "changed";
  beforeLine?: number;
  afterLine?: number;
  before?: string;
  after?: string;
}

/** Align small documents by LCS; line numbers are one-based on each side. */
export function diffLines(before: string[], after: string[]): LineHunk[] {
  const lengths = Array.from({ length: before.length + 1 }, () => new Uint32Array(after.length + 1));
  for (let i = before.length - 1; i >= 0; i--) {
    for (let j = after.length - 1; j >= 0; j--) {
      lengths[i][j] = before[i] === after[j]
        ? 1 + lengths[i + 1][j + 1]
        : Math.max(lengths[i + 1][j], lengths[i][j + 1]);
    }
  }

  const hunks: LineHunk[] = [];
  let i = 0;
  let j = 0;
  while (i < before.length || j < after.length) {
    if (i < before.length && j < after.length && before[i] === after[j]) {
      hunks.push({ kind: "same", beforeLine: i + 1, afterLine: j + 1, before: before[i++], after: after[j++] });
      continue;
    }
    // Collect one edit run between shared lines, pairing removals/additions
    // in order as changed lines. Extra lines count as individual edits.
    const removed: LineHunk[] = [];
    const added: LineHunk[] = [];
    while ((i < before.length || j < after.length)
      && !(i < before.length && j < after.length && before[i] === after[j])) {
      if (i < before.length && (j === after.length || lengths[i + 1][j] >= lengths[i][j + 1])) {
        removed.push({ kind: "removed", beforeLine: i + 1, before: before[i++] });
      } else {
        added.push({ kind: "added", afterLine: j + 1, after: after[j++] });
      }
    }
    for (let k = 0; k < Math.max(removed.length, added.length); k++) {
      hunks.push(removed[k] && added[k]
        ? { ...removed[k], ...added[k], kind: "changed" }
        : removed[k] ?? added[k]);
    }
  }
  return hunks;
}
