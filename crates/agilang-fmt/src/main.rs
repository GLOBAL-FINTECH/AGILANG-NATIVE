use anyhow::{Context, Result};
use std::{env, fs};

/// Trims trailing whitespace and ensures a final newline. The file's existing
/// line-ending style (LF or CRLF) is preserved, so checkouts that convert line
/// endings (e.g. Git on Windows) are not reported as unformatted.
fn format_source(input: &str) -> String {
    let eol = if input.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = input
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join(eol);
    if !out.ends_with(eol) {
        out.push_str(eol);
    }
    out
}

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let path = args
        .next()
        .context("usage: agilang-fmt <file.agi> [--check]")?;
    let check = args.any(|a| a == "--check");
    let text = fs::read_to_string(&path)?;
    let formatted = format_source(&text);
    if check {
        anyhow::ensure!(text == formatted, "{} is not formatted", path);
        println!("formatted: {}", path);
    } else {
        fs::write(&path, formatted)?;
        println!("formatted: {}", path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::format_source;

    #[test]
    fn formatted_lf_and_crlf_sources_are_stable() {
        let lf = "fn main() -> i32:\n    return 0\n";
        let crlf = "fn main() -> i32:\r\n    return 0\r\n";
        assert_eq!(format_source(lf), lf);
        assert_eq!(format_source(crlf), crlf);
    }

    #[test]
    fn trims_trailing_whitespace_and_adds_final_newline() {
        assert_eq!(
            format_source("fn main():   \n    return 0"),
            "fn main():\n    return 0\n"
        );
        assert_eq!(
            format_source("fn main():  \r\n    return 0"),
            "fn main():\r\n    return 0\r\n"
        );
    }
}
