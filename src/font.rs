//! Digit fonts for the big time display.
//!
//! A font is a way of drawing a string: figlet-style ASCII art, solid block
//! "pixel" digits, or just plain text. `FONTS` lists them from tallest to
//! shortest; the user picks one with `f`, and if it doesn't fit the window the
//! next shorter font that does is used instead.

use ratatui::text::Line;

type Glyphs = fn(char) -> Option<&'static [&'static str]>;

enum Kind {
    /// Hand-drawn ASCII art, one `&str` per row.
    Art(Glyphs),
    /// 3x5 bitmap digits drawn with full blocks, each pixel `sx` columns by
    /// `sy` rows.
    Pixel { sx: usize, sy: usize },
    /// The same bitmaps packed two pixel rows per text row with half blocks.
    HalfBlock,
    /// Ordinary text.
    Plain,
}

pub struct Font {
    pub name: &'static str,
    pub height: usize,
    /// Blank columns between glyphs.
    gap: usize,
    kind: Kind,
}

/// Tallest first. `f` cycles in this order.
pub static FONTS: [Font; 7] = [
    Font {
        name: "pixel-xl",
        height: 10,
        gap: 4,
        kind: Kind::Pixel { sx: 4, sy: 2 },
    },
    Font {
        name: "colossal",
        height: 8,
        gap: 0,
        kind: Kind::Art(big_glyph),
    },
    Font {
        name: "standard",
        height: 5,
        gap: 1,
        kind: Kind::Art(standard_glyph),
    },
    Font {
        name: "pixel",
        height: 5,
        gap: 2,
        kind: Kind::Pixel { sx: 2, sy: 1 },
    },
    Font {
        name: "mini",
        height: 3,
        gap: 1,
        kind: Kind::Art(mini_glyph),
    },
    Font {
        name: "half",
        height: 3,
        gap: 1,
        kind: Kind::HalfBlock,
    },
    Font {
        name: "plain",
        height: 1,
        gap: 0,
        kind: Kind::Plain,
    },
];

/// Picks the size for you: digits at most half the window height, in ASCII
/// art. The default, so the time is neither huge nor tiny on any window.
pub const AUTO: &str = "auto";
pub const DEFAULT: &str = AUTO;

/// The fonts `auto` chooses between, tallest first.
const AUTO_FAMILY: [&str; 4] = ["colossal", "standard", "mini", "plain"];

/// Rows the hint line below the digits needs.
const HINT_ROWS: usize = 2;

/// Number of things `f` cycles through: `auto` and every font.
pub fn choices() -> usize {
    FONTS.len() + 1
}

/// Name of choice `sel`: 0 is `auto`, then the fonts in `FONTS` order.
pub fn name_at(sel: usize) -> &'static str {
    match sel.checked_sub(1) {
        None => AUTO,
        Some(i) => FONTS[i].name,
    }
}

/// The choice called `name`, or the default's if there is none.
pub fn index_of(name: &str) -> usize {
    if name == AUTO {
        return 0;
    }
    FONTS
        .iter()
        .position(|f| f.name == name)
        .map_or(0, |i| i + 1)
}

fn named(name: &str) -> &'static Font {
    FONTS
        .iter()
        .find(|f| f.name == name)
        .unwrap_or(&FONTS[FONTS.len() - 1])
}

/// The font to draw `text` with in an area of `width` x `height`.
///
/// For `auto` that is the tallest ASCII font that is at most half the height,
/// rounded up (and at least the 3-row one, if it fits). For a named font it is that font if it fits, otherwise the next
/// shorter one that does. Plain text is the last resort either way.
pub fn pick(text: &str, width: usize, height: usize, sel: usize) -> &'static Font {
    let fits =
        |f: &&Font| f.supports(text) && f.width(text) <= width && f.height + HINT_ROWS <= height;
    let plain = &FONTS[FONTS.len() - 1];
    match sel.checked_sub(1) {
        None => {
            // half the height, rounded up, but never less than the smallest art font
            let target = height.div_ceil(2).max(named("mini").height);
            AUTO_FAMILY
                .iter()
                .map(|n| named(n))
                .find(|f| f.height <= target && fits(f))
        }
        Some(i) => FONTS[i..].iter().find(fits),
    }
    .unwrap_or(plain)
}

impl Font {
    fn supports_char(&self, c: char) -> bool {
        match self.kind {
            Kind::Art(glyphs) => glyphs(c).is_some(),
            Kind::Pixel { .. } | Kind::HalfBlock => bitmap(c).is_some(),
            Kind::Plain => true,
        }
    }

    /// Whether the font can draw every character of `text`.
    pub fn supports(&self, text: &str) -> bool {
        text.chars().all(|c| self.supports_char(c))
    }

    /// The rows of one glyph, unpadded. Unknown characters draw as a space.
    fn glyph(&self, c: char) -> Vec<String> {
        let c = if self.supports_char(c) { c } else { ' ' };
        match self.kind {
            Kind::Art(glyphs) => glyphs(c)
                .unwrap_or(&[])
                .iter()
                .map(|r| r.to_string())
                .collect(),
            Kind::Pixel { sx, sy } => {
                let rows = bitmap(c).unwrap_or(&[]);
                rows.iter()
                    .flat_map(|row| {
                        let line: String = row
                            .chars()
                            .flat_map(|p| {
                                std::iter::repeat_n(if p == '#' { '\u{2588}' } else { ' ' }, sx)
                            })
                            .collect();
                        std::iter::repeat_n(line, sy)
                    })
                    .collect()
            }
            Kind::HalfBlock => {
                let rows: Vec<Vec<bool>> = bitmap(c)
                    .unwrap_or(&[])
                    .iter()
                    .map(|r| r.chars().map(|p| p == '#').collect())
                    .collect();
                let at = |y: usize, x: usize| {
                    rows.get(y).and_then(|r| r.get(x)).copied().unwrap_or(false)
                };
                let width = rows.first().map_or(0, Vec::len);
                (0..self.height)
                    .map(|line| {
                        (0..width)
                            .map(|x| match (at(2 * line, x), at(2 * line + 1, x)) {
                                (true, true) => '\u{2588}',
                                (true, false) => '\u{2580}',
                                (false, true) => '\u{2584}',
                                (false, false) => ' ',
                            })
                            .collect()
                    })
                    .collect()
            }
            Kind::Plain => vec![c.to_string()],
        }
    }

    /// Display width in columns of `text` rendered in this font.
    pub fn width(&self, text: &str) -> usize {
        let glyphs: usize = text.chars().map(|c| glyph_width(&self.glyph(c))).sum();
        glyphs + self.gap * text.chars().count().saturating_sub(1)
    }

    /// Render `text` as `height` rows of equal width, glyphs side by side.
    pub fn rows(&self, text: &str) -> Vec<String> {
        let mut rows = vec![String::new(); self.height];
        for (i, c) in text.chars().enumerate() {
            let g = self.glyph(c);
            let w = glyph_width(&g);
            for (row, line) in rows.iter_mut().zip(&g) {
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

fn glyph_width(g: &[String]) -> usize {
    g.iter().map(|l| l.chars().count()).max().unwrap_or(0)
}

/// 3x5 pixel digits (wider for a few letters). `#` is a lit pixel.
fn bitmap(c: char) -> Option<&'static [&'static str]> {
    Some(match c {
        '0' => &["###", "#.#", "#.#", "#.#", "###"],
        '1' => &[".#.", "##.", ".#.", ".#.", "###"],
        '2' => &["###", "..#", "###", "#..", "###"],
        '3' => &["###", "..#", "###", "..#", "###"],
        '4' => &["#.#", "#.#", "###", "..#", "..#"],
        '5' => &["###", "#..", "###", "..#", "###"],
        '6' => &["###", "#..", "###", "#.#", "###"],
        '7' => &["###", "..#", "..#", "..#", "..#"],
        '8' => &["###", "#.#", "###", "#.#", "###"],
        '9' => &["###", "#.#", "###", "..#", "###"],
        '.' => &[".", ".", ".", ".", "#"],
        ':' => &[".", "#", ".", "#", "."],
        '+' => &["...", ".#.", "###", ".#.", "..."],
        '-' => &["...", "...", "###", "...", "..."],
        'D' => &["###.", "#..#", "#..#", "#..#", "###."],
        'N' => &["#...#", "##..#", "#.#.#", "#..##", "#...#"],
        'F' => &["###", "#..", "##.", "#..", "#.."],
        '(' => &[".#", "#.", "#.", "#.", ".#"],
        ')' => &["#.", ".#", ".#", ".#", "#."],
        's' => &["###", "#..", "###", "..#", "###"],
        ' ' => &["..", "..", "..", "..", ".."],
        _ => return None,
    })
}

fn standard_glyph(c: char) -> Option<&'static [&'static str]> {
    Some(match c {
        '0' => &[
            r#"  ___  "#,
            r#" / _ \ "#,
            r#"| | | |"#,
            r#"| |_| |"#,
            r#" \___/ "#,
        ],
        '1' => &[r#" _ "#, r#"/ |"#, r#"| |"#, r#"| |"#, r#"|_|"#],
        '2' => &[
            r#" ____  "#,
            r#"|___ \ "#,
            r#"  __) |"#,
            r#" / __/ "#,
            r#"|_____|"#,
        ],
        '3' => &[
            r#" _____ "#,
            r#"|___ / "#,
            r#"  |_ \ "#,
            r#" ___) |"#,
            r#"|____/ "#,
        ],
        '4' => &[
            r#" _  _   "#,
            r#"| || |  "#,
            r#"| || |_ "#,
            r#"|__   _|"#,
            r#"   |_|  "#,
        ],
        '5' => &[
            r#" ____  "#,
            r#"| ___| "#,
            r#"|___ \ "#,
            r#" ___) |"#,
            r#"|____/ "#,
        ],
        '6' => &[
            r#"  __   "#,
            r#" / /_  "#,
            r#"| '_ \ "#,
            r#"| (_) |"#,
            r#" \___/ "#,
        ],
        '7' => &[
            r#" _____ "#,
            r#"|___  |"#,
            r#"   / / "#,
            r#"  / /  "#,
            r#" /_/   "#,
        ],
        '8' => &[
            r#"  ___  "#,
            r#" ( _ ) "#,
            r#" / _ \ "#,
            r#"| (_) |"#,
            r#" \___/ "#,
        ],
        '9' => &[
            r#"  ___  "#,
            r#" / _ \ "#,
            r#"| (_) |"#,
            r#" \__, |"#,
            r#"   /_/ "#,
        ],
        '.' => &[r#"   "#, r#"   "#, r#"   "#, r#" _ "#, r#"(_)"#],
        ':' => &[r#" _ "#, r#"(_)"#, r#"   "#, r#" _ "#, r#"(_)"#],
        '+' => &[
            r#"       "#,
            r#"   _   "#,
            r#" _| |_ "#,
            r#"|_   _|"#,
            r#"  |_|  "#,
        ],
        '-' => &[
            r#"        "#,
            r#" _____  "#,
            r#"|_____| "#,
            r#"        "#,
            r#"        "#,
        ],
        'D' => &[
            r#" ____  "#,
            r#"|  _ \ "#,
            r#"| | | |"#,
            r#"| |_| |"#,
            r#"|____/ "#,
        ],
        'N' => &[
            r#" _   _ "#,
            r#"| \ | |"#,
            r#"|  \| |"#,
            r#"| |\  |"#,
            r#"|_| \_|"#,
        ],
        'F' => &[
            r#" _____ "#,
            r#"|  ___|"#,
            r#"| |_   "#,
            r#"|  _|  "#,
            r#"|_|    "#,
        ],
        '(' => &[r#"  __"#, r#" / /"#, r#"| | "#, r#"| | "#, r#" \_\"#],
        ')' => &[r#"__  "#, r#"\ \ "#, r#" | |"#, r#" | |"#, r#" /_/"#],
        's' => &[
            r#"      "#,
            r#" ___  "#,
            r#"/ __| "#,
            r#"\__ \ "#,
            r#"|___/ "#,
        ],
        ' ' => &[r#"   "#, r#"   "#, r#"   "#, r#"   "#, r#"   "#],
        _ => return None,
    })
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
            r#"     "#, r#"     "#, r#"     "#, r#"     "#, r#"     "#, r#"     "#, r#" 888 "#,
            r#" 888 "#,
        ],
        ':' => &[
            r#"     "#, r#"     "#, r#" 888 "#, r#" 888 "#, r#"     "#, r#" 888 "#, r#" 888 "#,
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
            r#"     "#, r#"     "#, r#"     "#, r#"     "#, r#"     "#, r#"     "#, r#"     "#,
            r#"     "#,
        ],
        _ => return None,
    })
}

fn mini_glyph(c: char) -> Option<&'static [&'static str]> {
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
    fn fonts_are_listed_tallest_first_with_unique_names() {
        assert!(FONTS.windows(2).all(|w| w[0].height >= w[1].height));
        for (i, f) in FONTS.iter().enumerate() {
            assert!(FONTS[..i].iter().all(|g| g.name != f.name), "{}", f.name);
        }
        assert_eq!(FONTS[FONTS.len() - 1].name, "plain");
    }

    #[test]
    fn default_is_auto_and_unknown_names_fall_back_to_it() {
        assert_eq!(name_at(index_of(DEFAULT)), DEFAULT);
        assert_eq!(index_of(AUTO), 0);
        assert_eq!(index_of("no-such-font"), index_of(DEFAULT));
        assert_eq!(index_of(""), index_of(DEFAULT));
    }

    #[test]
    fn choices_round_trip_through_their_names() {
        assert_eq!(choices(), FONTS.len() + 1);
        assert_eq!(name_at(0), "auto");
        for sel in 0..choices() {
            assert_eq!(index_of(name_at(sel)), sel);
        }
        assert!((0..choices()).all(|a| (0..a).all(|b| name_at(a) != name_at(b))));
    }

    #[test]
    fn auto_family_names_all_exist() {
        for n in AUTO_FAMILY {
            assert_eq!(named(n).name, n);
        }
        assert!(
            AUTO_FAMILY
                .windows(2)
                .all(|w| named(w[0]).height >= named(w[1]).height)
        );
    }

    #[test]
    fn every_font_draws_every_character_at_its_height() {
        for f in &FONTS {
            for c in CHARS.chars() {
                assert!(f.supports_char(c), "{} {c:?}", f.name);
                assert_eq!(f.glyph(c).len(), f.height, "{} {c:?}", f.name);
            }
        }
    }

    #[test]
    fn rows_are_equal_width_and_match_width() {
        for f in &FONTS {
            for text in ["12.34", "1:02.34+", "DNF", "DNF(12.34)", "15", "0"] {
                let rows = f.rows(text);
                assert_eq!(rows.len(), f.height, "{}", f.name);
                let w = f.width(text);
                assert!(
                    rows.iter().all(|r| r.chars().count() == w),
                    "{} {text:?}",
                    f.name
                );
            }
        }
    }

    #[test]
    fn every_digit_looks_different_in_every_font() {
        for f in &FONTS {
            let drawn: Vec<Vec<String>> = "0123456789"
                .chars()
                .map(|c| f.rows(&c.to_string()))
                .collect();
            for i in 0..drawn.len() {
                for j in 0..i {
                    assert_ne!(drawn[i], drawn[j], "{}: {i} looks like {j}", f.name);
                }
            }
        }
    }

    #[test]
    fn pixel_fonts_use_blocks_and_ascii_fonts_do_not() {
        for f in &FONTS {
            let ink: String = f.rows("12.34").concat();
            let has_blocks = ink.chars().any(|c| ('\u{2580}'..='\u{259f}').contains(&c));
            let blocky = matches!(f.kind, Kind::Pixel { .. } | Kind::HalfBlock);
            assert_eq!(has_blocks, blocky, "{}", f.name);
            if !blocky {
                assert!(ink.is_ascii(), "{}", f.name);
            }
        }
    }

    #[test]
    fn plain_font_is_the_text_itself() {
        let plain = &FONTS[FONTS.len() - 1];
        assert_eq!(plain.rows("12.34"), vec!["12.34"]);
        assert_eq!(plain.rows("solving..."), vec!["solving..."]);
        assert!(plain.supports("anything at all"));
    }

    #[test]
    fn art_fonts_refuse_text_they_have_no_glyphs_for() {
        for f in &FONTS[..FONTS.len() - 1] {
            assert!(!f.supports("solving..."), "{}", f.name);
        }
    }

    #[test]
    fn unknown_chars_render_blank() {
        for f in &FONTS[..FONTS.len() - 1] {
            let rows = f.rows("?");
            assert!(rows.iter().all(|r| r.trim().is_empty()), "{}", f.name);
        }
    }

    #[test]
    fn empty_text_is_empty_rows() {
        for f in &FONTS {
            assert!(f.rows("").iter().all(|r| r.is_empty()));
            assert_eq!(f.width(""), 0);
        }
    }

    #[test]
    fn pick_keeps_the_chosen_font_when_it_fits() {
        let text = "12.34";
        for (i, f) in FONTS.iter().enumerate() {
            let w = f.width(text);
            assert_eq!(pick(text, w, f.height + HINT_ROWS, i + 1).name, f.name);
        }
    }

    #[test]
    fn pick_steps_down_when_the_window_is_too_small() {
        let text = "12.34";
        let colossal = index_of("colossal");
        // not tall enough for colossal (8 + 2): the next font that fits is used
        let h = named("colossal").height + HINT_ROWS - 1;
        let picked = pick(text, 500, h, colossal);
        assert!(picked.height + HINT_ROWS <= h);
        assert!(picked.height < named("colossal").height);
        // not wide enough for colossal either
        let narrow = named("colossal").width(text) - 1;
        assert!(pick(text, narrow, 50, colossal).width(text) <= narrow);
    }

    #[test]
    fn pick_never_goes_back_up() {
        // choosing a small font in a huge window must not pick a bigger one
        assert_eq!(pick("12.34", 500, 100, index_of("mini")).name, "mini");
    }

    #[test]
    fn pick_ends_at_plain_text() {
        for sel in 0..choices() {
            assert_eq!(pick("12.34", 3, 3, sel).name, "plain", "{sel}");
            assert_eq!(pick("12.34", 0, 0, sel).name, "plain", "{sel}");
            assert_eq!(pick("solving...", 500, 100, sel).name, "plain", "{sel}");
        }
    }

    #[test]
    fn auto_scales_with_the_window_but_stays_moderate() {
        let at = |h| pick("12.34", 300, h, 0);
        assert_eq!(at(5).name, "mini");
        assert_eq!(at(8).name, "mini");
        assert_eq!(at(9).name, "standard");
        // a 24-row terminal leaves 13 rows for the time
        assert_eq!(at(12).name, "standard");
        assert_eq!(at(13).name, "standard");
        assert_eq!(at(14).name, "standard");
        assert_eq!(at(15).name, "colossal");
        // never taller than half the window (rounded up), and never a giant
        for h in 5..120 {
            let f = at(h);
            assert!(
                f.height <= h.div_ceil(2) || f.name == "mini",
                "{h}: {}",
                f.name
            );
            assert!(f.height <= 8, "{h}");
        }
    }

    #[test]
    fn auto_shrinks_for_a_narrow_window() {
        let colossal = named("colossal").width("12.34");
        assert_eq!(pick("12.34", colossal, 50, 0).name, "colossal");
        assert_ne!(pick("12.34", colossal - 1, 50, 0).name, "colossal");
    }

    #[test]
    fn render_gives_one_line_per_row() {
        for f in &FONTS {
            assert_eq!(f.render("7").len(), f.height);
        }
    }
}
