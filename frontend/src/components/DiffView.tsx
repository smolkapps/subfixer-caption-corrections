import type { WordOp } from "../lib/worddiff";

/** Render a word-level diff with inserts highlighted green, deletes red. */
export function DiffView({ ops }: { ops: WordOp[] }) {
  if (ops.length === 0) {
    return <span className="diff-empty">No content yet.</span>;
  }
  return (
    <p className="diff-view" data-testid="diff-view">
      {ops.map((op, idx) => {
        if (op.op === "equal") {
          return <span key={idx}>{op.word} </span>;
        }
        if (op.op === "insert") {
          return (
            <ins key={idx} className="diff-ins" data-testid="diff-ins">
              {op.word}{" "}
            </ins>
          );
        }
        return (
          <del key={idx} className="diff-del" data-testid="diff-del">
            {op.word}{" "}
          </del>
        );
      })}
    </p>
  );
}
