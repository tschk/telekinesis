use ansi_to_tui::IntoText;
use crepuscularity_tui::ratatui::text::{Line, Text};
use glamour::{Renderer, Style};

pub fn render(markdown: &str, width: usize) -> Vec<Line<'static>> {
    let mut styles = Style::TokyoNight.config();
    styles.document.margin = Some(0);
    styles.document.style.block_prefix.clear();
    styles.document.style.block_suffix.clear();
    let output = Renderer::new()
        .with_style_config(styles)
        .with_word_wrap(width.max(1))
        .with_preserved_newlines(true)
        .render(markdown);

    let mut lines = output
        .into_text()
        .unwrap_or_else(|_| Text::raw(output))
        .lines;
    // The renderer preserves the model's stray `\n\n\n` runs and adds its own
    // block gaps on top, which produced double-height gaps between paragraphs
    // and lists. Collapse every run of blank rows to a single row and drop
    // trailing blanks so blocks sit one blank line apart at most.
    let mut collapsed: Vec<Line<'static>> = Vec::with_capacity(lines.len());
    let mut last_blank = false;
    for line in lines.drain(..) {
        let blank = line.to_string().trim().is_empty();
        if blank && last_blank {
            continue;
        }
        last_blank = blank;
        collapsed.push(line);
    }
    while collapsed
        .last()
        .is_some_and(|line| line.to_string().trim().is_empty())
    {
        collapsed.pop();
    }
    collapsed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_stray_blank_runs_between_paragraphs() {
        let lines = render(
            "Summary line.\n\n\nMain threads:\n\n\n\n1. First item\n2. Second item\n\n\nNothing is committed.",
            80,
        );
        let content: Vec<String> = lines.iter().map(Line::to_string).collect();
        assert_eq!(content[0], "Summary line.");
        assert_eq!(content[1], "");
        assert_eq!(content[2], "Main threads:");
        assert_eq!(content[3], "");
        assert_eq!(content[4], "1. First item");
        assert!(
            !content
                .windows(2)
                .any(|pair| pair[0].is_empty() && pair[1].is_empty()),
            "no two consecutive blank lines: {content:?}"
        );
        assert!(content.last().is_some_and(|line| !line.trim().is_empty()));
    }

    #[test]
    fn keeps_blank_lines_inside_code_fences() {
        let lines = render(
            "Before.\n\n\n```rust\nfn a() {}\n\nfn b() {}\n```\n\n\nAfter.",
            80,
        );
        let content: Vec<String> = lines.iter().map(Line::to_string).collect();
        let fence_start = content
            .iter()
            .position(|line| line.contains("fn a() {}"))
            .expect("code block rendered");
        assert!(
            content
                .iter()
                .skip(fence_start)
                .any(|line| line.trim().is_empty() && line.starts_with(' ')),
            "the blank line inside the fence survives: {content:?}"
        );
        assert!(content.iter().any(|line| line == "After."));
    }

    #[test]
    fn retains_full_multiline_content() {
        let lines = render(
            "# Status\n\nFirst paragraph survives.\n\n- alpha\n- omega\n\nFinal line survives.",
            80,
        );
        let content = lines
            .iter()
            .map(Line::to_string)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(content.contains("Status"));
        assert!(content.contains("First paragraph survives."));
        assert!(content.contains("alpha"));
        assert!(content.contains("omega"));
        assert!(content.contains("Final line survives."));
    }

    #[test]
    fn styles_and_wraps_markdown() {
        let lines = render(
            "This is **important** and deliberately long enough to wrap across several terminal lines.",
            24,
        );

        assert!(lines.len() > 1);
        assert!(lines
            .iter()
            .flat_map(|line| &line.spans)
            .any(|span| span.style.fg.is_some() || !span.style.add_modifier.is_empty()));
    }
}
