use std::path::{Path, PathBuf};

pub fn crash_report_path(line: &str) -> Option<PathBuf> {
    let marker = "crash-reports/";
    let start = line.find(marker)?;
    let value = &line[start..];
    let end = value
        .find(|character: char| character.is_whitespace())
        .unwrap_or(value.len());
    Some(Path::new(&value[..end]).to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::crash_report_path;

    #[test]
    fn finds_crash_report_path_in_log_line() {
        assert_eq!(
            crash_report_path("Crash report saved to crash-reports/crash-2026-01-01.txt"),
            Some(std::path::PathBuf::from(
                "crash-reports/crash-2026-01-01.txt"
            ))
        );
        assert!(crash_report_path("normal output").is_none());
    }
}
