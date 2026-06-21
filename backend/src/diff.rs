//! Word-level caption diffing and change scoring.
//!
//! The core product value is detecting *what* a fixer changed between an
//! auto-generated caption and their corrected version, and quantifying it
//! (number of words changed) for the leaderboard and the optional
//! pay-per-N-words reward economy.

use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};

/// A single word-level edit operation in a caption correction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum WordOp {
    /// Word present in both versions, unchanged.
    Equal { word: String },
    /// Word present only in the corrected version (added by the fixer).
    Insert { word: String },
    /// Word present only in the original version (removed by the fixer).
    Delete { word: String },
}

/// The full computed diff between an original caption and a correction,
/// together with the change statistics used for scoring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptionDiff {
    pub ops: Vec<WordOp>,
    /// Number of words inserted (present in corrected, absent in original).
    pub words_inserted: u32,
    /// Number of words deleted (present in original, absent in corrected).
    pub words_deleted: u32,
    /// The headline metric: total words changed. A substitution of one word
    /// for another counts as a single changed word, not two. Pure
    /// insertions/deletions each count once.
    pub words_changed: u32,
}

/// Tokenize caption text into normalized words.
///
/// Whitespace is collapsed; we keep word casing and surrounding punctuation
/// because caption corrections frequently *are* punctuation/casing fixes and
/// those should be surfaced as real changes.
pub fn tokenize(text: &str) -> Vec<String> {
    text.split_whitespace().map(|s| s.to_string()).collect()
}

/// Compute a word-level diff and the change statistics.
///
/// `words_changed` is computed as `max(inserted, deleted)` per the standard
/// definition of substitution distance at the word granularity: replacing
/// "teh" with "the" is one changed word, not an insert plus a delete. Adding
/// three new words with nothing removed is three changed words.
pub fn diff_captions(original: &str, corrected: &str) -> CaptionDiff {
    let orig_words = tokenize(original);
    let corr_words = tokenize(corrected);

    // `similar`'s `from_slices` diffs slices of references (`&[&T]`), so build
    // borrowed views over the owned token vectors.
    let orig_refs: Vec<&str> = orig_words.iter().map(String::as_str).collect();
    let corr_refs: Vec<&str> = corr_words.iter().map(String::as_str).collect();

    let diff = TextDiff::from_slices(&orig_refs, &corr_refs);

    let mut ops = Vec::new();
    let mut inserted = 0u32;
    let mut deleted = 0u32;

    for change in diff.iter_all_changes() {
        let word = change.value().to_string();
        match change.tag() {
            ChangeTag::Equal => ops.push(WordOp::Equal { word }),
            ChangeTag::Insert => {
                inserted += 1;
                ops.push(WordOp::Insert { word });
            }
            ChangeTag::Delete => {
                deleted += 1;
                ops.push(WordOp::Delete { word });
            }
        }
    }

    let words_changed = inserted.max(deleted);

    CaptionDiff {
        ops,
        words_inserted: inserted,
        words_deleted: deleted,
        words_changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_collapses_whitespace() {
        assert_eq!(
            tokenize("  hello   world \n foo "),
            vec!["hello", "world", "foo"]
        );
        assert!(tokenize("").is_empty());
        assert!(tokenize("    ").is_empty());
    }

    #[test]
    fn identical_text_has_no_changes() {
        let d = diff_captions("the quick brown fox", "the quick brown fox");
        assert_eq!(d.words_changed, 0);
        assert_eq!(d.words_inserted, 0);
        assert_eq!(d.words_deleted, 0);
        assert!(d.ops.iter().all(|o| matches!(o, WordOp::Equal { .. })));
    }

    #[test]
    fn single_word_substitution_is_one_change() {
        // "teh" -> "the": one delete + one insert = one changed word.
        let d = diff_captions("teh quick brown fox", "the quick brown fox");
        assert_eq!(d.words_inserted, 1);
        assert_eq!(d.words_deleted, 1);
        assert_eq!(d.words_changed, 1);
    }

    #[test]
    fn pure_insertion_counts_each_added_word() {
        let d = diff_captions("hello world", "hello there big world");
        assert_eq!(d.words_inserted, 2); // "there" and "big"
        assert_eq!(d.words_deleted, 0);
        assert_eq!(d.words_changed, 2);
    }

    #[test]
    fn pure_deletion_counts_each_removed_word() {
        let d = diff_captions("um hello uh world", "hello world");
        assert_eq!(d.words_deleted, 2); // "um" and "uh"
        assert_eq!(d.words_inserted, 0);
        assert_eq!(d.words_changed, 2);
    }

    #[test]
    fn substitution_uses_max_not_sum() {
        // Replace two words with three: 3 inserted, 2 deleted -> max = 3.
        let d = diff_captions("foo bar baz", "one two three baz");
        assert_eq!(d.words_inserted, 3);
        assert_eq!(d.words_deleted, 2);
        assert_eq!(d.words_changed, 3);
    }

    #[test]
    fn empty_original_is_all_insertions() {
        let d = diff_captions("", "brand new caption");
        assert_eq!(d.words_inserted, 3);
        assert_eq!(d.words_deleted, 0);
        assert_eq!(d.words_changed, 3);
    }

    #[test]
    fn empty_correction_is_all_deletions() {
        let d = diff_captions("delete all of this", "");
        assert_eq!(d.words_deleted, 4);
        assert_eq!(d.words_inserted, 0);
        assert_eq!(d.words_changed, 4);
    }

    #[test]
    fn both_empty_is_no_change() {
        let d = diff_captions("", "");
        assert_eq!(d.words_changed, 0);
        assert!(d.ops.is_empty());
    }

    #[test]
    fn punctuation_and_casing_are_real_changes() {
        // "hello world" -> "Hello, world." : both tokens change.
        let d = diff_captions("hello world", "Hello, world.");
        assert_eq!(d.words_changed, 2);
    }

    #[test]
    fn ops_preserve_corrected_words_in_order() {
        let d = diff_captions("a c", "a b c");
        let corrected: Vec<&str> = d
            .ops
            .iter()
            .filter_map(|o| match o {
                WordOp::Equal { word } | WordOp::Insert { word } => Some(word.as_str()),
                WordOp::Delete { .. } => None,
            })
            .collect();
        assert_eq!(corrected, vec!["a", "b", "c"]);
    }
}
