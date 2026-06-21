import { useEffect, useState } from "react";
import { getLeaderboard, setAnonymity, type LeaderboardEntry } from "../api";

export function LeaderboardPage() {
  const [rows, setRows] = useState<LeaderboardEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  // Opt-out control.
  const [name, setName] = useState("");
  const [anon, setAnon] = useState(true);
  const [optMsg, setOptMsg] = useState<string | null>(null);

  async function refresh() {
    setLoading(true);
    setError(null);
    try {
      setRows(await getLeaderboard(100));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    refresh();
  }, []);

  async function onToggleAnon(e: React.FormEvent) {
    e.preventDefault();
    setOptMsg(null);
    try {
      await setAnonymity(name.trim(), anon);
      setOptMsg(
        anon
          ? `You will now appear as "Anonymous Fixer".`
          : `Your name will now be shown publicly.`,
      );
      await refresh();
    } catch (e) {
      setOptMsg((e as Error).message);
    }
  }

  return (
    <section>
      <h2>Top fixers</h2>
      {loading && <p>Loading…</p>}
      {error && <p className="status err" role="alert">{error}</p>}
      {!loading && !error && rows.length === 0 && (
        <p>No corrections yet. Be the first to fix a caption!</p>
      )}
      {rows.length > 0 && (
        <table className="leaderboard">
          <thead>
            <tr>
              <th>#</th>
              <th>Fixer</th>
              <th>Words changed</th>
              <th>Corrections</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.fixer_id ?? `anon-${r.rank}`}>
                <td>{r.rank}</td>
                <td>{r.display_name}</td>
                <td>{r.words_changed}</td>
                <td>{r.corrections}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      <div className="card">
        <h3>Privacy: appear anonymously</h3>
        <p className="muted">
          Opt out of public attribution. Your stats still count toward the
          rankings, but your name is replaced with “Anonymous Fixer”.
        </p>
        <form onSubmit={onToggleAnon} className="row">
          <label>
            Your fixer name
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="caption_hero"
              required
            />
          </label>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={anon}
              onChange={(e) => setAnon(e.target.checked)}
            />
            Hide my name
          </label>
          <button type="submit" disabled={name.trim() === ""}>
            Save privacy setting
          </button>
        </form>
        {optMsg && <p className="status" role="status">{optMsg}</p>}
      </div>
    </section>
  );
}
