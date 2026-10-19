//! Prints every digit font so you can compare them: `cargo run --example fonts`.
//! Pass your own text to try it: `cargo run --example fonts -- 1:02.34+`.

#[path = "../src/font.rs"]
#[allow(dead_code)]
mod font;

fn main() {
    let text = std::env::args().nth(1).unwrap_or_else(|| "13.54".into());
    println!("auto: sizes the digits to the window (ASCII art, at most 8 rows)\n");
    for f in font::FONTS.iter() {
        if !f.supports(&text) {
            println!("{}: can't draw {text:?}\n", f.name);
            continue;
        }
        println!(
            "{} ({} row{})",
            f.name,
            f.height,
            if f.height == 1 { "" } else { "s" }
        );
        for row in f.rows(&text) {
            println!("{}", row.trim_end());
        }
        println!();
    }
}
