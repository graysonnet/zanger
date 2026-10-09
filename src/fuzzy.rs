/// Rank case-insensitive subsequences by adjacent letters, word boundaries,
/// and filename matches. Normalize separators for queries on any platform.
pub fn score(path: &str, query: &str) -> Option<i64> {
    let path = path.replace('\\', "/").to_lowercase();
    let query = query.replace('\\', "/").to_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    let chars: Vec<_> = path.chars().collect();
    let mut previous = vec![i64::MIN / 4; chars.len()];
    for (qi, wanted) in query.chars().enumerate() {
        let mut current = vec![i64::MIN / 4; chars.len()];
        let mut best = i64::MIN / 4;
        for (index, &c) in chars.iter().enumerate() {
            if c == wanted {
                let boundary =
                    index == 0 || matches!(chars[index - 1], '/' | '_' | '-' | '.' | ' ');
                let base = if qi == 0 {
                    -(index as i64)
                } else {
                    let consecutive = if index > 0 {
                        previous[index - 1] + 12
                    } else {
                        i64::MIN / 4
                    };
                    consecutive.max(best - index as i64)
                };
                current[index] = base + 10 + if boundary { 16 } else { 0 };
            }
            best = best.max(previous[index] + index as i64);
        }
        previous = current;
    }
    let best = previous.into_iter().max()?;
    if best < -1_000_000 {
        return None;
    }
    let filename = path.rsplit('/').next().unwrap_or(&path);
    Some(
        best + if filename == query {
            200
        } else if filename.starts_with(&query) {
            80
        } else {
            0
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranks_names_and_supports_gaps_unicode_and_separators() {
        assert!(score("src/PowerShellRunner.ps1", "psrn").is_some());
        assert!(score("src/目录/文件.ps1", "目文").is_some());
        assert_eq!(
            score("src\\file.rs", "SRC/F"),
            score("src/file.rs", "src/f")
        );
        assert!(score("other/config.rs", "config.rs") > score("config/long.rs", "config.rs"));
        for (path, query) in [
            ("src/first.rs", "zxy"),
            ("ab", "aa"),
            ("ab", "ba"),
            ("file.txt", " "),
        ] {
            assert!(score(path, query).is_none());
        }
    }
}
