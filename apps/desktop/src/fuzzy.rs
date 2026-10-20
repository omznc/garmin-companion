//! Match a half-typed query against a label.
//!
//! The query is a subsequence of the label: `snc` finds "Sync now". Letters in
//! a row and letters that start a word score more, so the best match sorts first.

/// Points for a subsequence match, or `None` if the letters are not all there.
///
/// Both arguments must be lowercase. The caller folds the case.
pub fn subsequence(needle: &str, haystack: &str) -> Option<i32> {
    let mut points = 0;
    let mut run = 0;
    let mut chars = haystack.char_indices();

    for want in needle.chars() {
        loop {
            let (i, got) = chars.next()?;
            if got == want {
                points += 1;
                let starts_word = i == 0
                    || haystack[..i]
                        .chars()
                        .next_back()
                        .is_some_and(|c| !c.is_alphanumeric());
                points += run * 2 + i32::from(starts_word) * 3;
                run += 1;
                break;
            }
            run = 0;
        }
    }
    // A short label beats a long one when the rest is equal.
    Some(points - (haystack.chars().count() as i32) / 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_that_are_not_all_there_are_not_a_match() {
        assert!(subsequence("xyz", "sync now").is_none());
        // The letters are there, but not in this order.
        assert!(subsequence("wns", "sync now").is_none());
    }

    #[test]
    fn letters_in_a_row_beat_letters_scattered_about() {
        let together = subsequence("sync", "sync now").unwrap();
        let apart = subsequence("sync", "s-y-n-c").unwrap();
        assert!(together > apart, "{together} should beat {apart}");
    }

    #[test]
    fn a_word_start_beats_a_letter_in_the_middle() {
        let starting = subsequence("n", "now").unwrap();
        let buried = subsequence("n", "sync").unwrap();
        assert!(starting > buried, "{starting} should beat {buried}");
    }

    #[test]
    fn an_empty_query_matches_anything() {
        assert!(subsequence("", "anything").is_some());
    }
}
