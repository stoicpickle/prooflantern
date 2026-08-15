use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub fn truncate(value: &str, width: usize) -> String {
    if UnicodeWidthStr::width(value) <= width {
        return value.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "…".into();
    }
    let mut rendered = String::new();
    let target = width - 1;
    for grapheme in value.graphemes(true) {
        if UnicodeWidthStr::width(rendered.as_str()) + UnicodeWidthStr::width(grapheme) > target {
            break;
        }
        rendered.push_str(grapheme);
    }
    rendered.push('…');
    rendered
}

pub fn middle_truncate(value: &str, width: usize) -> String {
    if UnicodeWidthStr::width(value) <= width {
        return value.to_owned();
    }
    if width < 4 {
        return truncate(value, width);
    }
    let left_width = (width - 1) / 2;
    let right_width = width - 1 - left_width;
    let left = truncate_without_marker(value, left_width, false);
    let right = truncate_without_marker(value, right_width, true);
    format!("{left}…{right}")
}

fn truncate_without_marker(value: &str, width: usize, from_end: bool) -> String {
    let graphemes: Vec<_> = value.graphemes(true).collect();
    let iterator: Box<dyn Iterator<Item = &&str>> = if from_end {
        Box::new(graphemes.iter().rev())
    } else {
        Box::new(graphemes.iter())
    };
    let mut selected = Vec::new();
    let mut used = 0;
    for grapheme in iterator {
        let grapheme_width = UnicodeWidthStr::width(*grapheme);
        if used + grapheme_width > width {
            break;
        }
        used += grapheme_width;
        selected.push(*grapheme);
    }
    if from_end {
        selected.reverse();
    }
    selected.concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_respects_terminal_cells_and_graphemes() {
        assert_eq!(truncate("recipe", 6), "recipe");
        assert_eq!(truncate("recipe", 4), "rec…");
        assert_eq!(truncate("資料視覚化", 5), "資料…");
        assert_eq!(middle_truncate("src/deep/storage.rs", 12), "src/d…age.rs");
    }
}
