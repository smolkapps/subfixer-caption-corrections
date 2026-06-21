//! Leaderboard ranking and anonymity logic.
//!
//! Kept as pure functions over plain data so the ranking rules (sort order,
//! tie-breaking, anonymization, opt-out) are testable without a database.

use serde::{Deserialize, Serialize};

/// Aggregated stats for one fixer, as read out of the store before ranking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixerStats {
    pub fixer_id: String,
    pub display_name: String,
    /// Whether this fixer has opted to appear anonymously on the public board.
    pub anonymous: bool,
    pub corrections: u32,
    pub words_changed: u32,
}

/// A single ranked row ready to serialize to the public leaderboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub rank: u32,
    /// Stable id is omitted for anonymous fixers so they cannot be correlated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixer_id: Option<String>,
    pub display_name: String,
    pub corrections: u32,
    pub words_changed: u32,
}

/// The label shown for a fixer who has opted out of public attribution.
pub const ANON_LABEL: &str = "Anonymous Fixer";

/// Rank fixers for the public leaderboard.
///
/// Ordering: most `words_changed` first; ties broken by more `corrections`,
/// then by `display_name` ascending (case-insensitive) for stable, deterministic
/// output. Anonymous fixers keep their stats and rank but have their name
/// replaced and their id withheld.
///
/// `limit` caps the number of returned rows (0 means "no limit").
pub fn rank(mut stats: Vec<FixerStats>, limit: usize) -> Vec<LeaderboardEntry> {
    stats.sort_by(|a, b| {
        b.words_changed
            .cmp(&a.words_changed)
            .then_with(|| b.corrections.cmp(&a.corrections))
            .then_with(|| {
                a.display_name
                    .to_lowercase()
                    .cmp(&b.display_name.to_lowercase())
            })
            .then_with(|| a.fixer_id.cmp(&b.fixer_id))
    });

    let iter = stats.into_iter().enumerate().map(|(i, s)| {
        let (display_name, fixer_id) = if s.anonymous {
            (ANON_LABEL.to_string(), None)
        } else {
            (s.display_name, Some(s.fixer_id))
        };
        LeaderboardEntry {
            rank: (i as u32) + 1,
            fixer_id,
            display_name,
            corrections: s.corrections,
            words_changed: s.words_changed,
        }
    });

    if limit == 0 {
        iter.collect()
    } else {
        iter.take(limit).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(id: &str, name: &str, anon: bool, corr: u32, words: u32) -> FixerStats {
        FixerStats {
            fixer_id: id.into(),
            display_name: name.into(),
            anonymous: anon,
            corrections: corr,
            words_changed: words,
        }
    }

    #[test]
    fn ranks_by_words_changed_desc() {
        let board = rank(
            vec![
                fx("a", "Alice", false, 5, 10),
                fx("b", "Bob", false, 1, 99),
                fx("c", "Cara", false, 3, 50),
            ],
            0,
        );
        let names: Vec<&str> = board.iter().map(|e| e.display_name.as_str()).collect();
        assert_eq!(names, vec!["Bob", "Cara", "Alice"]);
        assert_eq!(board[0].rank, 1);
        assert_eq!(board[2].rank, 3);
    }

    #[test]
    fn ties_break_on_corrections_then_name() {
        // Same words_changed; Bob has more corrections so he wins; then Alice vs
        // Adam on name with equal corrections -> Adam first.
        let board = rank(
            vec![
                fx("1", "Alice", false, 2, 20),
                fx("2", "Bob", false, 9, 20),
                fx("3", "Adam", false, 2, 20),
            ],
            0,
        );
        let names: Vec<&str> = board.iter().map(|e| e.display_name.as_str()).collect();
        assert_eq!(names, vec!["Bob", "Adam", "Alice"]);
    }

    #[test]
    fn anonymous_fixer_is_masked_but_keeps_stats_and_rank() {
        let board = rank(
            vec![
                fx("pub", "TopFixer", false, 1, 100),
                fx("secret", "RealName", true, 1, 50),
            ],
            0,
        );
        assert_eq!(board[1].display_name, ANON_LABEL);
        assert_eq!(board[1].fixer_id, None);
        assert_eq!(board[1].words_changed, 50);
        assert_eq!(board[1].rank, 2);
        // Non-anonymous fixer keeps id + name.
        assert_eq!(board[0].fixer_id.as_deref(), Some("pub"));
        assert_eq!(board[0].display_name, "TopFixer");
    }

    #[test]
    fn limit_caps_rows() {
        let board = rank(
            vec![
                fx("a", "A", false, 1, 3),
                fx("b", "B", false, 1, 2),
                fx("c", "C", false, 1, 1),
            ],
            2,
        );
        assert_eq!(board.len(), 2);
        assert_eq!(board[0].display_name, "A");
        assert_eq!(board[1].display_name, "B");
    }

    #[test]
    fn empty_input_yields_empty_board() {
        assert!(rank(vec![], 0).is_empty());
        assert!(rank(vec![], 10).is_empty());
    }

    #[test]
    fn anonymous_entries_are_not_correlatable_by_id() {
        // Two anonymous fixers must both be unidentifiable.
        let board = rank(
            vec![
                fx("s1", "Secret One", true, 1, 5),
                fx("s2", "Secret Two", true, 1, 4),
            ],
            0,
        );
        assert!(board.iter().all(|e| e.fixer_id.is_none()));
        assert!(board.iter().all(|e| e.display_name == ANON_LABEL));
    }
}
