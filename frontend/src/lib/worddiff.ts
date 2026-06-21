// Client-side word-level diff, mirroring the backend's definitions so the live
// preview matches what the server will store. Implemented with a standard LCS
// so substitutions show as delete+insert and the changed-word count is
// max(inserted, deleted).

export type WordOp =
  | { op: "equal"; word: string }
  | { op: "insert"; word: string }
  | { op: "delete"; word: string };

export interface WordDiff {
  ops: WordOp[];
  wordsInserted: number;
  wordsDeleted: number;
  wordsChanged: number;
}

export function tokenize(text: string): string[] {
  return text.split(/\s+/).filter((t) => t.length > 0);
}

export function diffWords(original: string, corrected: string): WordDiff {
  const a = tokenize(original);
  const b = tokenize(corrected);

  // LCS length table.
  const n = a.length;
  const m = b.length;
  const lcs: number[][] = Array.from({ length: n + 1 }, () =>
    new Array<number>(m + 1).fill(0),
  );
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      if (a[i] === b[j]) lcs[i][j] = lcs[i + 1][j + 1] + 1;
      else lcs[i][j] = Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }

  const ops: WordOp[] = [];
  let inserted = 0;
  let deleted = 0;
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      ops.push({ op: "equal", word: a[i] });
      i++;
      j++;
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
      ops.push({ op: "delete", word: a[i] });
      deleted++;
      i++;
    } else {
      ops.push({ op: "insert", word: b[j] });
      inserted++;
      j++;
    }
  }
  while (i < n) {
    ops.push({ op: "delete", word: a[i] });
    deleted++;
    i++;
  }
  while (j < m) {
    ops.push({ op: "insert", word: b[j] });
    inserted++;
    j++;
  }

  return {
    ops,
    wordsInserted: inserted,
    wordsDeleted: deleted,
    wordsChanged: Math.max(inserted, deleted),
  };
}
