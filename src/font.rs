//! Big-digit fonts: a figlet "colossal"-style font and a compact 3-row
//! fallback for narrow terminals.

use ratatui::text::Line;

pub struct Font {
    pub height: usize,
    /// Blank columns between glyphs.
    gap: usize,
    glyphs: fn(char) -> Option<&'static [&'static str]>,
}

pub const BIG: Font = Font { height: 8, gap: 0, glyphs: big_glyph };
pub const SMALL: Font = Font { height: 3, gap: 1, glyphs: small_glyph };

impl Font {
    /// The glyph for `c`, or a blank for characters the font doesn't have.
    fn glyph(&self, c: char) -> &'static [&'static str] {
        (self.glyphs)(c).unwrap_or_else(|| (self.glyphs)(' ').unwrap())
    }

    /// Whether the font has a glyph for every character of `text`.
    pub fn supports(&self, text: &str) -> bool {
        text.chars().all(|c| (self.glyphs)(c).is_some())
    }

    /// Display width in columns of `text` rendered in this font.
    pub fn width(&self, text: &str) -> usize {
        let mut total = 0;
        for (i, c) in text.chars().enumerate() {
            total += glyph_width(self.glyph(c));
            if i > 0 {
                total += self.gap;
            }
        }
        total
    }

    /// Render `text` as `height` rows of equal width, glyphs side by side.
    pub fn rows(&self, text: &str) -> Vec<String> {
        let mut rows = vec![String::new(); self.height];
        for (i, c) in text.chars().enumerate() {
            let g = self.glyph(c);
            let w = glyph_width(g);
            for (row, line) in rows.iter_mut().zip(g) {
                if i > 0 {
                    row.extend(std::iter::repeat_n(' ', self.gap));
                }
                row.push_str(line);
                row.extend(std::iter::repeat_n(' ', w - line.chars().count()));
            }
        }
        rows
    }

    pub fn render(&self, text: &str) -> Vec<Line<'static>> {
        self.rows(text).into_iter().map(Line::from).collect()
    }
}

/// The big font if `text` fits in `max_width` columns, otherwise the small one.
pub fn pick(text: &str, max_width: usize) -> &'static Font {
    static BIG_FONT: Font = BIG;
    static SMALL_FONT: Font = SMALL;
    if BIG_FONT.width(text) <= max_width { &BIG_FONT } else { &SMALL_FONT }
}

fn glyph_width(g: &[&str]) -> usize {
    g.iter().map(|l| l.chars().count()).max().unwrap_or(0)
}

fn big_glyph(c: char) -> Option<&'static [&'static str]> {
    Some(match c {
        '0' => &[
            r#"  .d8888b.  "#,
            r#" d88P  Y88b "#,
            r#" 888    888 "#,
            r#" 888    888 "#,
            r#" 888    888 "#,
            r#" 888    888 "#,
            r#" Y88b  d88P "#,
            r#"  "Y8888P"  "#,
        ],
        '1' => &[
            r#"  d888   "#,
            r#" d8888   "#,
            r#"   888   "#,
            r#"   888   "#,
            r#"   888   "#,
            r#"   888   "#,
            r#"   888   "#,
            r#" 8888888 "#,
        ],
        '2' => &[
            r#"  .d8888b.  "#,
            r#" d88P  Y88b "#,
            r#"        888 "#,
            r#"      .d88P "#,
            r#"  .od888P"  "#,
            r#" d88P"      "#,
            r#" 888"       "#,
            r#" 888888888  "#,
        ],
        '3' => &[
            r#"  .d8888b.  "#,
            r#" d88P  Y88b "#,
            r#"      .d88P "#,
            r#"     8888"  "#,
            r#"      "Y8b. "#,
            r#" 888    888 "#,
            r#" Y88b  d88P "#,
            r#"  "Y8888P"  "#,
        ],
        '4' => &[
            r#"     d8888  "#,
            r#"    d8P888  "#,
            r#"   d8P 888  "#,
            r#"  d8P  888  "#,
            r#" d88   888  "#,
            r#" 8888888888 "#,
            r#"       888  "#,
            r#"       888  "#,
        ],
        '5' => &[
            r#" 888888888  "#,
            r#" 888        "#,
            r#" 888        "#,
            r#" 8888888b.  "#,
            r#"      "Y88b "#,
            r#"        888 "#,
            r#" Y88b  d88P "#,
            r#"  "Y8888P"  "#,
        ],
        '6' => &[
            r#"  .d8888b.  "#,
            r#" d88P  Y88b "#,
            r#" 888        "#,
            r#" 888d888b.  "#,
            r#" 888P "Y88b "#,
            r#" 888    888 "#,
            r#" Y88b  d88P "#,
            r#"  "Y8888P"  "#,
        ],
        '7' => &[
            r#" 8888888888 "#,
            r#"       d88P "#,
            r#"      d88P  "#,
            r#"     d88P   "#,
            r#"  88888888  "#,
            r#"   d88P     "#,
            r#"  d88P      "#,
            r#" d88P       "#,
        ],
        '8' => &[
            r#"  .d8888b.  "#,
            r#" d88P  Y88b "#,
            r#" Y88b. d88P "#,
            r#"  "Y88888"  "#,
            r#" .d8P""Y8b. "#,
            r#" 888    888 "#,
            r#" Y88b  d88P "#,
            r#"  "Y8888P"  "#,
        ],
        '9' => &[
            r#"  .d8888b.  "#,
            r#" d88P  Y88b "#,
            r#" 888    888 "#,
            r#" Y88b. d888 "#,
            r#"  "Y888P888 "#,
            r#"        888 "#,
            r#" Y88b  d88P "#,
            r#"  "Y8888P"  "#,
        ],
        '.' => &[
            r#"     "#,
            r#"     "#,
            r#"     "#,
            r#"     "#,
            r#"     "#,
            r#"     "#,
            r#" 888 "#,
            r#" 888 "#,
        ],
        ':' => &[
            r#"     "#,
            r#"     "#,
            r#" 888 "#,
            r#" 888 "#,
            r#"     "#,
            r#" 888 "#,
            r#" 888 "#,
            r#"     "#,
        ],
        '+' => &[
            r#"           "#,
            r#"    888    "#,
            r#"    888    "#,
            r#" 888888888 "#,
            r#"    888    "#,
            r#"    888    "#,
            r#"           "#,
            r#"           "#,
        ],
        '-' => &[
            r#"            "#,
            r#"            "#,
            r#"            "#,
            r#" 8888888888 "#,
            r#"            "#,
            r#"            "#,
            r#"            "#,
            r#"            "#,
        ],
        'D' => &[
            r#" 8888888b.  "#,
            r#" 888  "Y88b "#,
            r#" 888    888 "#,
            r#" 888    888 "#,
            r#" 888    888 "#,
            r#" 888    888 "#,
            r#" 888  .d88P "#,
            r#" 8888888P"  "#,
        ],
        'N' => &[
            r#" 888b    888 "#,
            r#" 8888b   888 "#,
            r#" 88888b  888 "#,
            r#" 888Y88b 888 "#,
            r#" 888 Y88b888 "#,
            r#" 888  Y88888 "#,
            r#" 888   Y8888 "#,
            r#" 888    Y888 "#,
        ],
        'F' => &[
            r#" 8888888888 "#,
            r#" 888        "#,
            r#" 888        "#,
            r#" 8888888    "#,
            r#" 888        "#,
            r#" 888        "#,
            r#" 888        "#,
            r#" 888        "#,
        ],
        '(' => &[
            r#"   .d88 "#,
            r#"  d88P  "#,
            r#" d88P   "#,
            r#" 888    "#,
            r#" 888    "#,
            r#" Y88b   "#,
            r#"  Y88b  "#,
            r#"   "Y88 "#,
        ],
        ')' => &[
            r#" 88b.   "#,
            r#"   Y88b "#,
            r#"    Y88b"#,
            r#"     888"#,
            r#"     888"#,
            r#"    d88P"#,
            r#"   d88P "#,
            r#" 88P"   "#,
        ],
        's' => &[
            r#"           "#,
            r#"           "#,
            r#"           "#,
            r#" .d8888b   "#,
            r#" 88K       "#,
            r#" "Y8888b.  "#,
            r#"      X88  "#,
            r#"  88888P'  "#,
        ],
        ' ' => &[
            r#"     "#,
            r#"     "#,
            r#"     "#,
            r#"     "#,
            r#"     "#,
            r#"     "#,
            r#"     "#,
            r#"     "#,
        ],
        _ => return None,
    })
}

fn small_glyph(c: char) -> Option<&'static [&'static str]> {
    Some(match c {
        '0' => &[" _ ", "| |", "|_|"],
        '1' => &["   ", "  |", "  |"],
        '2' => &[" _ ", " _|", "|_ "],
        '3' => &[" _ ", " _|", " _|"],
        '4' => &["   ", "|_|", "  |"],
        '5' => &[" _ ", "|_ ", " _|"],
        '6' => &[" _ ", "|_ ", "|_|"],
        '7' => &[" _ ", "  |", "  |"],
        '8' => &[" _ ", "|_|", "|_|"],
        '9' => &[" _ ", "|_|", " _|"],
        '.' => &[" ", " ", "."],
        ':' => &[" ", ".", "."],
        '+' => &["   ", " + ", "   "],
        '-' => &["   ", " _ ", "   "],
        'D' => &[" _ ", "| \\", "|_/"],
        'N' => &["   ", "|\\|", "| |"],
        'F' => &[" _ ", "|_ ", "|  "],
        '(' => &[" /", "| ", " \\"],
        ')' => &["\\ ", " |", "/ "],
        's' => &["  ", " _", " _)"],
        ' ' => &["  ", "  ", "  "],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHARS: &str = "0123456789.:+-DNF() s";

    #[test]
    fn every_glyph_has_the_font_height() {
        for font in [&BIG, &SMALL] {
            for c in CHARS.chars() {
                assert_eq!(font.glyph(c).len(), font.height, "{c:?}");
            }
        }
    }

    #[test]
    fn rows_are_equal_width_and_match_width() {
        for font in [&BIG, &SMALL] {
            for text in ["12.34", "1:02.34+", "DNF", "DNF(12.34)", "15"] {
                let rows = font.rows(text);
                assert_eq!(rows.len(), font.height);
                let w = font.width(text);
                assert!(rows.iter().all(|r| r.chars().count() == w), "{text:?}");
            }
        }
    }

    #[test]
    fn unknown_chars_render_blank() {
        let rows = BIG.rows("?");
        assert_eq!(rows, BIG.rows(" "));
        assert!(rows.iter().all(|r| r.trim().is_empty()));
    }

    #[test]
    fn supports_checks_every_char() {
        assert!(BIG.supports("1:02.34+"));
        assert!(!BIG.supports("solving..."));
    }

    #[test]
    fn empty_text_is_empty_rows() {
        assert!(BIG.rows("").iter().all(|r| r.is_empty()));
        assert_eq!(BIG.width(""), 0);
    }

    #[test]
    fn pick_falls_back_when_narrow() {
        let t = "12.34";
        assert_eq!(pick(t, 200).height, 8);
        assert_eq!(pick(t, BIG.width(t)).height, 8);
        assert_eq!(pick(t, BIG.width(t) - 1).height, 3);
    }

    #[test]
    fn render_gives_one_line_per_row() {
        assert_eq!(BIG.render("7").len(), 8);
        assert_eq!(SMALL.render("7").len(), 3);
    }
}
