//! Preview an instance cover at the size the instances page can show it.
//!
//! ```text
//! cargo run --example cover -- path/to/icon.png
//! cargo run --example cover -- path/to/icon.png --plain
//! ```
//!
//! The coloured output is the real renderer's target: 34 columns × 17 rows of
//! `▀`, each cell carrying two square pixels through its foreground and
//! background. `--plain` prints luminance instead, which survives a pipe and
//! is what CI or a transcript can actually show.

use std::fmt::Write as _;
use std::path::PathBuf;

use tmcl::core::cover::Cover;

const COLUMNS: u32 = 34;
const ROWS: u32 = 17;
const BACKGROUND: [u8; 3] = [0x12, 0x10, 0x1C];

fn main() {
    let mut args = std::env::args_os().skip(1);
    let Some(raw) = args.next() else {
        eprintln!("usage: cargo run --example cover -- <file.png> [--plain]");
        std::process::exit(2);
    };
    let path = PathBuf::from(raw);
    let plain = args.any(|arg| arg.to_string_lossy() == "--plain");

    let source = match Cover::load(&path) {
        Ok(cover) => cover,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    };
    let cover = source.square().resized(COLUMNS, ROWS * 2);
    println!(
        "{}: {}x{} -> {}x{} ({}x{} cells)",
        path.display(),
        source.width(),
        source.height(),
        cover.width(),
        cover.height(),
        COLUMNS,
        ROWS
    );
    if plain {
        print_plain(&cover);
    } else {
        print_ansi(&cover);
    }
}

fn blend(pixel: [u8; 4], background: [u8; 3]) -> [u8; 3] {
    let alpha = u16::from(pixel[3]);
    let mix = |foreground: u8, background: u8| -> u8 {
        ((u16::from(foreground) * alpha + u16::from(background) * (255 - alpha) + 127) / 255) as u8
    };
    [
        mix(pixel[0], background[0]),
        mix(pixel[1], background[1]),
        mix(pixel[2], background[2]),
    ]
}

fn print_ansi(cover: &Cover) {
    let mut out = String::new();
    for row in 0..ROWS {
        for column in 0..COLUMNS {
            let top = blend(cover.pixel(column, row * 2), BACKGROUND);
            let bottom = blend(cover.pixel(column, row * 2 + 1), BACKGROUND);
            let _ = write!(
                out,
                "\x1b[38;2;{};{};{}m\x1b[48;2;{};{};{}m▀",
                top[0], top[1], top[2], bottom[0], bottom[1], bottom[2]
            );
        }
        out.push_str("\x1b[0m\n");
    }
    print!("{out}");
}

fn print_plain(cover: &Cover) {
    const RAMP: &[u8] = b" .:-=+*#%@";
    for row in 0..ROWS {
        let mut line = String::new();
        for column in 0..COLUMNS {
            let top = blend(cover.pixel(column, row * 2), BACKGROUND);
            let luminance = 0.2126 * f32::from(top[0])
                + 0.7152 * f32::from(top[1])
                + 0.0722 * f32::from(top[2]);
            let index = ((luminance / 255.0) * (RAMP.len() - 1) as f32).round() as usize;
            line.push(char::from(RAMP[index.min(RAMP.len() - 1)]));
        }
        println!("{line}");
    }
}
