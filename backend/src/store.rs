//! SQLite-backed persistence for fixers and caption corrections.
//!
//! Self-contained: uses the `bundled` rusqlite SQLite, defaults to an in-memory
//! database for tests and a file (`SUBFIXER_DB` env var, else `subfixer.db`) in
//! production.

use crate::diff::{diff_captions, CaptionDiff};
use crate::error::SubFixerError;
use crate::leaderboard::{rank, FixerStats, LeaderboardEntry};
use crate::videokey::make_key;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use uuid::Uuid;

/// A persisted caption correction with its computed diff stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correction {
    pub id: String,
    pub storage_key: String,
    pub video_id: String,
    pub start_sec: u32,
    pub original_text: String,
    pub corrected_text: String,
    pub words_changed: u32,
    pub fixer_id: String,
    pub fixer_name: String,
    pub created_at: i64,
}

/// Input payload for submitting a new correction.
#[derive(Debug, Clone, Deserialize)]
pub struct NewCorrection {
    pub video_url: String,
    pub start_sec: i64,
    pub original_text: String,
    pub corrected_text: String,
    pub fixer_name: String,
}

/// Thread-safe store handle. A single Mutex-guarded connection is plenty for
/// this app's write volume and keeps the in-memory test DB on one connection.
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Open (or create) a file-backed store and run migrations.
    pub fn open(path: &str) -> Result<Self, SubFixerError> {
        let conn = Connection::open(path)?;
        let store = Store {
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Open an in-memory store (used by tests).
    pub fn open_in_memory() -> Result<Self, SubFixerError> {
        let conn = Connection::open_in_memory()?;
        let store = Store {
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), SubFixerError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS fixers (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL UNIQUE,
                anonymous   INTEGER NOT NULL DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS corrections (
                id              TEXT PRIMARY KEY,
                storage_key     TEXT NOT NULL,
                video_id        TEXT NOT NULL,
                start_sec       INTEGER NOT NULL,
                original_text   TEXT NOT NULL,
                corrected_text  TEXT NOT NULL,
                words_changed   INTEGER NOT NULL,
                fixer_id        TEXT NOT NULL REFERENCES fixers(id),
                created_at      INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_corrections_key ON corrections(storage_key);
            CREATE INDEX IF NOT EXISTS idx_corrections_fixer ON corrections(fixer_id);
            "#,
        )?;
        Ok(())
    }

    /// Find an existing fixer by name or create a new one. Names are the public
    /// identity; we upsert on the unique name.
    fn upsert_fixer(conn: &Connection, name: &str) -> Result<String, SubFixerError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(SubFixerError::Validation("fixer_name is empty".into()));
        }
        if name.len() > 60 {
            return Err(SubFixerError::Validation(
                "fixer_name exceeds 60 chars".into(),
            ));
        }
        if let Some(id) = conn
            .query_row(
                "SELECT id FROM fixers WHERE name = ?1",
                params![name],
                |r| r.get::<_, String>(0),
            )
            .optional()?
        {
            return Ok(id);
        }
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO fixers (id, name, anonymous) VALUES (?1, ?2, 0)",
            params![id, name],
        )?;
        Ok(id)
    }

    /// Submit a correction. Validates inputs, rejects no-op corrections,
    /// computes the diff, persists, and returns the stored row.
    pub fn submit_correction(&self, input: NewCorrection) -> Result<Correction, SubFixerError> {
        let key = make_key(&input.video_url, input.start_sec)?;

        if input.corrected_text.trim().is_empty() {
            return Err(SubFixerError::Validation("corrected_text is empty".into()));
        }
        if input.original_text.len() > 5000 || input.corrected_text.len() > 5000 {
            return Err(SubFixerError::Validation(
                "caption text exceeds 5000 chars".into(),
            ));
        }

        let CaptionDiff { words_changed, .. } =
            diff_captions(&input.original_text, &input.corrected_text);
        if words_changed == 0 {
            return Err(SubFixerError::NoOpCorrection);
        }

        let conn = self.conn.lock().unwrap();
        let fixer_id = Self::upsert_fixer(&conn, &input.fixer_name)?;
        let id = Uuid::new_v4().to_string();
        let created_at = now_unix();

        conn.execute(
            r#"INSERT INTO corrections
               (id, storage_key, video_id, start_sec, original_text,
                corrected_text, words_changed, fixer_id, created_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
            params![
                id,
                key.as_storage_key(),
                key.video_id,
                key.start_sec,
                input.original_text,
                input.corrected_text,
                words_changed,
                fixer_id,
                created_at,
            ],
        )?;

        Ok(Correction {
            id,
            storage_key: key.as_storage_key(),
            video_id: key.video_id,
            start_sec: key.start_sec,
            original_text: input.original_text,
            corrected_text: input.corrected_text,
            words_changed,
            fixer_id,
            fixer_name: input.fixer_name.trim().to_string(),
            created_at,
        })
    }

    /// List all corrections for a given video, newest first. The video is
    /// identified by the same URL normalization used at submit time, so callers
    /// can pass any URL form.
    pub fn corrections_for_video(&self, video_url: &str) -> Result<Vec<Correction>, SubFixerError> {
        // start_sec=0 here only to satisfy make_key; we key the lookup on the
        // normalized video_id, across all timestamps.
        let key = make_key(video_url, 0)?;
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"SELECT c.id, c.storage_key, c.video_id, c.start_sec, c.original_text,
                      c.corrected_text, c.words_changed, c.fixer_id, f.name, c.created_at
               FROM corrections c JOIN fixers f ON f.id = c.fixer_id
               WHERE c.video_id = ?1
               ORDER BY c.start_sec ASC, c.created_at DESC"#,
        )?;
        let rows = stmt
            .query_map(params![key.video_id], row_to_correction)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// The global leaderboard of top fixers.
    pub fn leaderboard(&self, limit: usize) -> Result<Vec<LeaderboardEntry>, SubFixerError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            r#"SELECT f.id, f.name, f.anonymous,
                      COUNT(c.id) AS corrections,
                      COALESCE(SUM(c.words_changed), 0) AS words
               FROM fixers f LEFT JOIN corrections c ON c.fixer_id = f.id
               GROUP BY f.id
               HAVING corrections > 0"#,
        )?;
        let stats = stmt
            .query_map([], |r| {
                Ok(FixerStats {
                    fixer_id: r.get(0)?,
                    display_name: r.get(1)?,
                    anonymous: r.get::<_, i64>(2)? != 0,
                    corrections: r.get::<_, i64>(3)? as u32,
                    words_changed: r.get::<_, i64>(4)? as u32,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rank(stats, limit))
    }

    /// Toggle a fixer's public anonymity setting. Returns the new value.
    pub fn set_anonymous(&self, name: &str, anonymous: bool) -> Result<bool, SubFixerError> {
        let conn = self.conn.lock().unwrap();
        let affected = conn.execute(
            "UPDATE fixers SET anonymous = ?1 WHERE name = ?2",
            params![anonymous as i64, name.trim()],
        )?;
        if affected == 0 {
            return Err(SubFixerError::NotFound(format!("fixer '{}'", name.trim())));
        }
        Ok(anonymous)
    }
}

fn row_to_correction(r: &rusqlite::Row) -> rusqlite::Result<Correction> {
    Ok(Correction {
        id: r.get(0)?,
        storage_key: r.get(1)?,
        video_id: r.get(2)?,
        start_sec: r.get::<_, i64>(3)? as u32,
        original_text: r.get(4)?,
        corrected_text: r.get(5)?,
        words_changed: r.get::<_, i64>(6)? as u32,
        fixer_id: r.get(7)?,
        fixer_name: r.get(8)?,
        created_at: r.get(9)?,
    })
}

fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nc(url: &str, sec: i64, orig: &str, corr: &str, name: &str) -> NewCorrection {
        NewCorrection {
            video_url: url.into(),
            start_sec: sec,
            original_text: orig.into(),
            corrected_text: corr.into(),
            fixer_name: name.into(),
        }
    }

    #[test]
    fn submit_and_read_back() {
        let s = Store::open_in_memory().unwrap();
        let c = s
            .submit_correction(nc(
                "https://youtu.be/dQw4w9WgXcQ",
                12,
                "teh quick brown fox",
                "the quick brown fox",
                "Alice",
            ))
            .unwrap();
        assert_eq!(c.words_changed, 1);
        assert_eq!(c.video_id, "dQw4w9WgXcQ");
        assert_eq!(c.start_sec, 12);

        let list = s
            .corrections_for_video("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].corrected_text, "the quick brown fox");
        assert_eq!(list[0].fixer_name, "Alice");
    }

    #[test]
    fn different_url_forms_collide_on_same_video() {
        let s = Store::open_in_memory().unwrap();
        s.submit_correction(nc(
            "https://youtu.be/dQw4w9WgXcQ?si=abc",
            5,
            "helo",
            "hello",
            "Bob",
        ))
        .unwrap();
        s.submit_correction(nc(
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&feature=share",
            10,
            "wrld",
            "world",
            "Bob",
        ))
        .unwrap();
        let list = s
            .corrections_for_video("https://youtu.be/dQw4w9WgXcQ")
            .unwrap();
        assert_eq!(list.len(), 2);
        // Ordered by start_sec ascending.
        assert_eq!(list[0].start_sec, 5);
        assert_eq!(list[1].start_sec, 10);
    }

    #[test]
    fn noop_correction_is_rejected() {
        let s = Store::open_in_memory().unwrap();
        let err = s
            .submit_correction(nc(
                "https://youtu.be/dQw4w9WgXcQ",
                1,
                "same text here",
                "same text here",
                "Alice",
            ))
            .unwrap_err();
        assert!(matches!(err, SubFixerError::NoOpCorrection));
    }

    #[test]
    fn empty_correction_text_is_rejected() {
        let s = Store::open_in_memory().unwrap();
        let err = s
            .submit_correction(nc(
                "https://youtu.be/dQw4w9WgXcQ",
                1,
                "orig",
                "   ",
                "Alice",
            ))
            .unwrap_err();
        assert!(matches!(err, SubFixerError::Validation(_)));
    }

    #[test]
    fn empty_fixer_name_is_rejected() {
        let s = Store::open_in_memory().unwrap();
        let err = s
            .submit_correction(nc("https://youtu.be/dQw4w9WgXcQ", 1, "a b", "a c", "  "))
            .unwrap_err();
        assert!(matches!(err, SubFixerError::Validation(_)));
    }

    #[test]
    fn bad_timestamp_is_rejected() {
        let s = Store::open_in_memory().unwrap();
        let err = s
            .submit_correction(nc("https://youtu.be/dQw4w9WgXcQ", -5, "a", "b", "Alice"))
            .unwrap_err();
        assert!(matches!(err, SubFixerError::Validation(_)));
    }

    #[test]
    fn leaderboard_aggregates_and_ranks() {
        let s = Store::open_in_memory().unwrap();
        // Alice: 2 corrections, 1 + 2 = 3 words changed.
        s.submit_correction(nc("https://youtu.be/dQw4w9WgXcQ", 1, "teh", "the", "Alice"))
            .unwrap();
        s.submit_correction(nc(
            "https://youtu.be/dQw4w9WgXcQ",
            2,
            "helo wrld",
            "hello world",
            "Alice",
        ))
        .unwrap();
        // Bob: 1 correction, 1 word changed.
        s.submit_correction(nc("https://youtu.be/dQw4w9WgXcQ", 3, "fox", "FOX", "Bob"))
            .unwrap();

        let board = s.leaderboard(0).unwrap();
        assert_eq!(board.len(), 2);
        assert_eq!(board[0].display_name, "Alice");
        assert_eq!(board[0].words_changed, 3);
        assert_eq!(board[0].corrections, 2);
        assert_eq!(board[1].display_name, "Bob");
        assert_eq!(board[1].words_changed, 1);
    }

    #[test]
    fn anonymity_hides_name_on_leaderboard() {
        let s = Store::open_in_memory().unwrap();
        s.submit_correction(nc(
            "https://youtu.be/dQw4w9WgXcQ",
            1,
            "teh",
            "the",
            "Secret",
        ))
        .unwrap();
        s.set_anonymous("Secret", true).unwrap();
        let board = s.leaderboard(0).unwrap();
        assert_eq!(board.len(), 1);
        assert_eq!(board[0].display_name, crate::leaderboard::ANON_LABEL);
        assert_eq!(board[0].fixer_id, None);
        // stats still counted
        assert_eq!(board[0].words_changed, 1);
    }

    #[test]
    fn set_anonymous_unknown_fixer_errors() {
        let s = Store::open_in_memory().unwrap();
        let err = s.set_anonymous("ghost", true).unwrap_err();
        assert!(matches!(err, SubFixerError::NotFound(_)));
    }

    #[test]
    fn fixer_identity_is_reused_across_submissions() {
        let s = Store::open_in_memory().unwrap();
        let c1 = s
            .submit_correction(nc("https://youtu.be/dQw4w9WgXcQ", 1, "a", "b", "Same"))
            .unwrap();
        let c2 = s
            .submit_correction(nc("https://youtu.be/dQw4w9WgXcQ", 2, "c", "d", "Same"))
            .unwrap();
        assert_eq!(c1.fixer_id, c2.fixer_id);
    }
}
