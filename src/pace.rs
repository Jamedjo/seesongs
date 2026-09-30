//! Timings for plain lyrics, borrowed from timed recordings of other lengths.
//!
//! Lines within a section keep much the same spacing from one recording to the
//! next; what changes is the intro, the outro and the breaks between sections.
//! So the short gaps are copied as they are, and the long ones stretch or
//! shrink to fit the track.
//!
//! Recordings split and word lines differently ("brain about the" / "End of
//! the world"), so they are matched word by word rather than line by line.

/// A gap longer than this between two lines is a break and can stretch.
const BREAK_SECS: f64 = 12.0;

/// A recording must match at least this share of the words to be used.
const MIN_MATCHED: f64 = 0.5;

/// Rough time to sing one word, for placing words within a timed line.
const WORD_SECS: f64 = 0.35;

/// The last line starts at least this long before the track ends.
const MIN_OUTRO_SECS: f64 = 5.0;

/// Times for `lines` across a track of `length` seconds, or None when no
/// recording in `timed` shares enough words. Each of `timed` is a recording's
/// lines with their start times, and its listed length.
pub fn pace(
    lines: &[String],
    timed: &[(Vec<(f64, String)>, f64)],
    length: f64,
) -> Option<Vec<(f64, String)>> {
    let n = lines.len();
    if n < 2 || length <= 0.0 {
        return None;
    }
    // Each word of the plain lyrics, with its line and its place in that line.
    let target: Vec<(usize, usize, String)> = lines
        .iter()
        .enumerate()
        .flat_map(|(line, text)| {
            words(text)
                .into_iter()
                .enumerate()
                .map(move |(k, w)| (line, k, w))
        })
        .collect();
    let target_keys: Vec<&str> = target.iter().map(|(_, _, w)| w.as_str()).collect();

    // Every recording's measurement of each gap, and of the intro and outro.
    let mut gaps = vec![Vec::new(); n - 1];
    let mut intros = Vec::new();
    let mut outros = Vec::new();
    for (recording, duration) in timed {
        let sung = timed_words(recording);
        let sung_keys: Vec<&str> = sung.iter().map(|(w, _)| w.as_str()).collect();
        let pairs = common_words(&target_keys, &sung_keys);
        if (pairs.len() as f64) < target.len() as f64 * MIN_MATCHED {
            continue;
        }
        // A line starts at its first matched word, less the words before it.
        let mut starts = vec![None; n];
        for (a, b) in pairs {
            let (line, k, _) = &target[a];
            starts[*line].get_or_insert(sung[b].1 - *k as f64 * WORD_SECS);
        }
        for i in 0..n - 1 {
            if let (Some(from), Some(to)) = (starts[i], starts[i + 1]) {
                if to > from {
                    gaps[i].push(to - from);
                }
            }
        }
        if let Some(first) = starts[0] {
            intros.push(first.max(0.0));
        }
        if let Some(last) = starts[n - 1] {
            if *duration > last {
                outros.push(duration - last);
            }
        }
    }

    let measured: Vec<f64> = gaps.iter().filter_map(|g| median(g)).collect();
    if measured.is_empty() {
        return None;
    }
    // Gaps no recording measured get a typical line spacing.
    let sung_gaps: Vec<f64> = measured
        .iter()
        .copied()
        .filter(|g| *g <= BREAK_SECS)
        .collect();
    let typical = median(&sung_gaps).unwrap_or(BREAK_SECS / 2.0);
    let gaps: Vec<f64> = gaps.iter().map(|g| median(g).unwrap_or(typical)).collect();
    let intro = median(&intros).unwrap_or(typical);

    let fixed: f64 = gaps.iter().filter(|g| **g <= BREAK_SECS).sum();
    let stretchy: f64 = intro + gaps.iter().filter(|g| **g > BREAK_SECS).sum::<f64>();
    let last_line = fixed + stretchy;

    // Listed lengths are often wrong (studio timings filed under live albums),
    // so the outro only decides stretching when it leaves the track time over.
    let (fixed_scale, break_scale) = if last_line > length - MIN_OUTRO_SECS {
        let room = length - MIN_OUTRO_SECS - fixed;
        if room > 0.0 && stretchy > 0.0 {
            (1.0, room / stretchy)
        } else {
            let all = (length - MIN_OUTRO_SECS).max(0.0) / last_line;
            (all, all)
        }
    } else {
        match median(&outros) {
            Some(outro) if last_line + outro < length && stretchy > 0.0 => {
                (1.0, (length - outro - fixed) / stretchy)
            }
            _ => (1.0, 1.0),
        }
    };
    let scale = |g: f64| {
        g * if g > BREAK_SECS {
            break_scale
        } else {
            fixed_scale
        }
    };

    let mut at = intro * break_scale;
    let mut out = Vec::with_capacity(n);
    for (i, line) in lines.iter().enumerate() {
        out.push((at, line.clone()));
        if let Some(&g) = gaps.get(i) {
            at += scale(g);
        }
    }
    Some(out)
}

/// Lowercase letters and digits of each word, so punctuation and capitals
/// don't stop a match.
fn words(line: &str) -> Vec<String> {
    line.split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect()
        })
        .filter(|w: &String| !w.is_empty())
        .collect()
}

/// Every word of a timed recording with an estimated start, spread across its
/// line until the next line starts, at no more than twice WORD_SECS a word.
fn timed_words(recording: &[(f64, String)]) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for (i, (at, line)) in recording.iter().enumerate() {
        let line_words = words(line);
        let count = line_words.len() as f64;
        let longest = count * WORD_SECS * 2.0;
        let span = recording
            .get(i + 1)
            .map_or(longest, |(next, _)| (next - at).clamp(0.0, longest));
        for (k, word) in line_words.into_iter().enumerate() {
            out.push((word, at + span * k as f64 / count));
        }
    }
    out
}

/// Index pairs of the longest run of words the two share, in order.
fn common_words(a: &[&str], b: &[&str]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut len = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            len[i][j] = if a[i] == b[j] {
                len[i + 1][j + 1] + 1
            } else {
                len[i + 1][j].max(len[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut pairs) = (0, 0, Vec::new());
    while i < n && j < m {
        if a[i] == b[j] {
            pairs.push((i, j));
            i += 1;
            j += 1;
        } else if len[i + 1][j] >= len[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    pairs
}

fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    Some(if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|t| t.to_string()).collect()
    }

    fn timed(times: &[f64], texts: &[&str]) -> Vec<(f64, String)> {
        times
            .iter()
            .zip(texts)
            .map(|(t, l)| (*t, l.to_string()))
            .collect()
    }

    fn starts(paced: &[(f64, String)]) -> Vec<f64> {
        paced
            .iter()
            .map(|(t, _)| (t * 100.0).round() / 100.0)
            .collect()
    }

    const WORDS: [&str; 4] = ["One", "Two", "Three", "Four"];

    #[test]
    fn breaks_stretch_and_sung_gaps_keep_their_spacing() {
        // Intro 10s, lines 3s apart, a 30s break, then a 10s outro.
        let live = timed(&[10.0, 13.0, 43.0, 46.0], &WORDS);
        let paced = pace(&lines(&WORDS), &[(live, 56.0)], 106.0).unwrap();
        // 50s more track, shared by the 40s of intro and break: 2.25 times.
        assert_eq!(starts(&paced), vec![22.5, 25.5, 93.0, 96.0]);
    }

    #[test]
    fn a_wrong_listed_length_does_not_stretch() {
        let live = timed(&[10.0, 13.0, 43.0, 46.0], &WORDS);
        let paced = pace(&lines(&WORDS), &[(live, 800.0)], 60.0).unwrap();
        assert_eq!(starts(&paced), vec![10.0, 13.0, 43.0, 46.0]);
    }

    #[test]
    fn breaks_shrink_when_the_track_is_shorter() {
        let live = timed(&[10.0, 13.0, 43.0, 46.0], &WORDS);
        let paced = pace(&lines(&WORDS), &[(live, 56.0)], 31.0).unwrap();
        // The last line starts 5s before the end: 20s of room for 40s of breaks.
        assert_eq!(starts(&paced), vec![5.0, 8.0, 23.0, 26.0]);
    }

    #[test]
    fn lines_split_differently_still_match() {
        let live = timed(
            &[0.0, 4.0, 8.0],
            &[
                "I've got ideas in my brain about the",
                "End of the world that I won't even say",
                "When all",
            ],
        );
        let plain = lines(&[
            "I've got ideas in my brain",
            "About the end of the world that I won't even say",
            "When all",
        ]);
        let paced = pace(&plain, &[(live, 20.0)], 20.0).unwrap();
        // "About" is word 7 of 8 in the first live line: 4s * 6/8 in.
        assert_eq!(starts(&paced)[1], 3.0);
    }

    #[test]
    fn unrelated_recordings_are_ignored() {
        let live = timed(&[5.0, 8.0], &["something", "else"]);
        assert!(pace(&lines(&["One", "Two"]), &[(live, 20.0)], 40.0).is_none());
    }
}
