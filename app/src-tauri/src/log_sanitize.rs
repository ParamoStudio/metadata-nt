const MAX_LINE_CHARS: usize = 4000;

/// Sanitize untrusted process output (MAT2 stdout/stderr, diagnostics, file
/// names) for display in the technical log.
///
/// Guarantees (IMPLEMENTATION_PLAN Task 6):
/// - readable text and line structure are preserved;
/// - line endings normalized to `\n` (CR/CRLF tricks become visible newlines);
/// - ANSI/terminal escape sequences removed (CSI, OSC-with-BEL/ST, DCS, plain ESC);
/// - control characters (C0 except \n \t, DEL, C1) removed;
/// - invisible format characters removed: bidi overrides/isolates, zero-width
///   joiners/spaces, BOM, soft hyphen, tag characters — the text-spoofing and
///   data-smuggling vectors;
/// - HTML stays literal data (the frontend renders exclusively via textContent;
///   this function never escapes and never generates markup or links);
/// - lines longer than 4000 chars are truncated with a visible marker.
pub fn sanitize(input: &str) -> String {
    let mut out = String::with_capacity(input.len().min(64 * 1024));
    let mut chars = input.chars().peekable();
    let mut line_len = 0usize;
    let mut truncated_this_line = false;

    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
                line_len = 0;
                truncated_this_line = false;
            }
            '\n' => {
                out.push('\n');
                line_len = 0;
                truncated_this_line = false;
            }
            '\u{1b}' => {
                skip_escape(&mut chars);
            }
            '\t' => {
                if line_len < MAX_LINE_CHARS {
                    out.push('\t');
                    line_len += 1;
                } else if !truncated_this_line {
                    out.push_str(TRUNC_MARK);
                    truncated_this_line = true;
                }
            }
            c if is_stripped(c) => {}
            c => {
                if line_len < MAX_LINE_CHARS {
                    out.push(c);
                    line_len += 1;
                } else if !truncated_this_line {
                    out.push_str(TRUNC_MARK);
                    truncated_this_line = true;
                }
            }
        }
    }
    out
}

const TRUNC_MARK: &str = "…[truncated]";

/// Consume the remainder of an escape sequence starting after the ESC.
fn skip_escape(chars: &mut std::iter::Peekable<std::str::Chars>) {
    let Some(kind) = chars.peek().copied() else { return };
    match kind {
        // OSC / DCS / SOS / PM / APC: terminated by BEL or ST (ESC \)
        ']' | 'P' | 'X' | '^' | '_' => {
            chars.next();
            while let Some(c) = chars.next() {
                if c == '\u{7}' {
                    return;
                }
                if c == '\u{1b}' {
                    if chars.peek() == Some(&'\\') {
                        chars.next();
                    }
                    return;
                }
            }
        }
        // CSI and nF sequences: [intermediates 0x20-0x2F / params 0x30-0x3F]* final 0x40-0x7E
        _ => {
            chars.next();
            while let Some(&c) = chars.peek() {
                if ('\u{20}'..='\u{3f}').contains(&c) {
                    chars.next();
                } else {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        chars.next();
                    }
                    return;
                }
            }
        }
    }
}

/// SECURITY: beyond C0/C1 controls (is_control), strip Unicode Cf format
/// characters — invisible text used for filename spoofing (bidi overrides),
/// token splitting (zero-widths) and data smuggling (tag characters).
fn is_stripped(c: char) -> bool {
    if c.is_control() {
        return true;
    }
    matches!(c as u32,
        0x00AD                  // soft hyphen
        | 0x0600..=0x0605       // arabic number signs
        | 0x061C                // arabic letter mark
        | 0x06DD | 0x070F | 0x0890 | 0x0891 | 0x08E2
        | 0x180E                // mongolian vowel separator
        | 0x200B..=0x200F       // zero widths + LRM/RLM
        | 0x202A..=0x202E       // bidi embedding/override controls
        | 0x2060..=0x2064 | 0x2066..=0x206F
        | 0xFEFF                // BOM / ZWNBSP
        | 0xFFF9..=0xFFFB       // interlinear annotation
        | 0x110BD | 0x110CD
        | 0x1BCA0..=0x1BCA3 | 0x1D173..=0x1D17A
        | 0xE0001 | 0xE0020..=0xE007F  // tag characters
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi_csi_sequences_removed_text_kept() {
        assert_eq!(sanitize("\u{1b}[31mERROR\u{1b}[0m: bad file"), "ERROR: bad file");
        assert_eq!(sanitize("\u{1b}[1;33;40mWarning\u{1b}[m"), "Warning");
        assert_eq!(sanitize("\u{1b}[?25lhidden cursor\u{1b}[?25h"), "hidden cursor");
    }

    #[test]
    fn terminal_title_escapes_removed() {
        assert_eq!(sanitize("\u{1b}]0;evil-title\u{7}real text"), "real text");
        assert_eq!(sanitize("\u{1b}]2;evil\u{1b}\\real text"), "real text");
        assert_eq!(sanitize("\u{1b}P+q6b6e\u{1b}\\data"), "data");
        assert_eq!(sanitize("\u{1b}(Bfoo"), "foo");
        assert_eq!(sanitize("trailing\u{1b}"), "trailing");
        assert_eq!(sanitize("truncated\u{1b}[31"), "truncated");
    }

    #[test]
    fn carriage_return_tricks_become_visible_newlines() {
        assert_eq!(sanitize("OK\rFAILED!"), "OK\nFAILED!");
        assert_eq!(sanitize("a\r\nb"), "a\nb");
        assert_eq!(sanitize("clean\u{1b}[2K\r[-] injected"), "clean\n[-] injected");
    }

    #[test]
    fn control_characters_removed() {
        assert_eq!(sanitize("abc\u{8}d"), "abcd");
        assert_eq!(sanitize("a\u{0}b"), "ab");
        assert_eq!(sanitize("a\u{7f}b"), "ab");
        assert_eq!(sanitize("a\u{9b}b"), "ab");
        assert_eq!(sanitize("a\u{1}b\u{1f}c"), "abc");
    }

    #[test]
    fn ordinary_newlines_and_tabs_preserved() {
        assert_eq!(sanitize("line1\nline2\n"), "line1\nline2\n");
        assert_eq!(sanitize("col1\tcol2"), "col1\tcol2");
    }

    #[test]
    fn unicode_readable_text_preserved() {
        assert_eq!(sanitize("café-🎉.jpg — limpio ✓"), "café-🎉.jpg — limpio ✓");
        assert_eq!(sanitize("日本語のファイル名.pdf"), "日本語のファイル名.pdf");
    }

    #[test]
    fn invisible_spoofing_characters_removed() {
        assert_eq!(sanitize("photo\u{202E}gpj.txt"), "photogpj.txt");
        assert_eq!(sanitize("a\u{200B}b\u{FEFF}c"), "abc");
        assert_eq!(sanitize("safe\u{202E}\u{202D}mixed"), "safemixed");
        assert_eq!(sanitize("x\u{E0041}\u{E007F}y"), "xy");
        assert_eq!(sanitize("a\u{AD}b"), "ab");
    }

    #[test]
    fn html_stays_literal_data_never_escaped_or_linked() {
        let payload = "<img src=x onerror=alert(1)>.jpg";
        assert_eq!(sanitize(payload), payload);
        let script = "<script>alert('xss')</script>.pdf";
        assert_eq!(sanitize(script), script);
        let url = "see https://example.com/path here";
        assert_eq!(sanitize(url), url);
        let out = sanitize("<a href=x>y</a> & \"quoted\"");
        assert_eq!(out, "<a href=x>y</a> & \"quoted\"");
        assert!(!out.contains("&lt;") && !out.contains("&amp;"));
    }

    #[test]
    fn long_lines_truncated_with_marker() {
        let long = "A".repeat(10_000);
        let out = sanitize(&format!("start {} end\nsecond", long));
        let first = out.lines().next().unwrap();
        assert!(first.starts_with("start AAA"));
        assert!(first.ends_with(TRUNC_MARK));
        assert!(first.len() < MAX_LINE_CHARS + TRUNC_MARK.len() + 16);
        assert_eq!(out.lines().nth(1).unwrap(), "second");
    }

    #[test]
    fn sanitizer_is_idempotent_on_clean_text() {
        let text = "[20:41:03] Inspecting report.docx\n[20:41:04] Cleaning";
        assert_eq!(sanitize(&sanitize(text)), sanitize(text));
        assert_eq!(sanitize(text), text);
    }
}
