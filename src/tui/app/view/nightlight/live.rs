//! The warm accent curve used by the `live` theme.
//!
//! This is a clock, not a sunrise calculation: the screen simply gets warmer as
//! the day ends. Two smoothstep ramps move the colour temperature between a
//! candle-lit 2400 K and a neutral 4300 K; everything in between is flat.
//!
//! Clean-room notice: the black-body approximation below is a textbook fit
//! reimplemented from the published formula, not lifted from `nightlightd`.

use ratatui::style::Color;

use super::tone::{Rgb, to_color};

/// Candle orange, used from 21:00 to 05:00.
pub const NIGHT_KELVIN: u32 = 2400;
/// Neutral warm gold, used from 08:00 to 17:00.
pub const DAY_KELVIN: u32 = 4300;

const MORNING_START: f64 = 5.0;
const MORNING_END: f64 = 8.0;
const EVENING_START: f64 = 17.0;
const EVENING_END: f64 = 21.0;

/// Approximate a black body at `kelvin` as sRGB.
pub fn kelvin_to_rgb(kelvin: u32) -> Rgb {
    let temperature = f64::from(kelvin.clamp(1000, 40_000)) / 100.0;
    let red = if temperature <= 66.0 {
        255.0
    } else {
        329.698_727_446 * (temperature - 60.0).powf(-0.133_204_759_2)
    };
    let green = if temperature <= 66.0 {
        99.470_802_586_1 * temperature.ln() - 161.119_568_166_1
    } else {
        288.122_169_528_3 * (temperature - 60.0).powf(-0.075_514_849_2)
    };
    let blue = if temperature >= 66.0 {
        255.0
    } else if temperature <= 19.0 {
        0.0
    } else {
        138.517_731_223_1 * (temperature - 10.0).ln() - 305.044_792_730_7
    };
    let channel = |value: f64| value.round().clamp(0.0, 255.0) as u8;
    (channel(red), channel(green), channel(blue))
}

fn smoothstep(progress: f64) -> f64 {
    let t = progress.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Colour temperature for a local time given as minutes since midnight.
pub fn kelvin_at(minute: u32) -> u32 {
    let hour = f64::from(minute % 1440) / 60.0;
    let kelvin = if (MORNING_START..MORNING_END).contains(&hour) {
        lerp(
            NIGHT_KELVIN,
            DAY_KELVIN,
            smoothstep((hour - MORNING_START) / (MORNING_END - MORNING_START)),
        )
    } else if (EVENING_START..EVENING_END).contains(&hour) {
        lerp(
            DAY_KELVIN,
            NIGHT_KELVIN,
            smoothstep((hour - EVENING_START) / (EVENING_END - EVENING_START)),
        )
    } else if (MORNING_END..EVENING_START).contains(&hour) {
        f64::from(DAY_KELVIN)
    } else {
        f64::from(NIGHT_KELVIN)
    };
    kelvin.round() as u32
}

fn lerp(from: u32, to: u32, progress: f64) -> f64 {
    f64::from(from) + (f64::from(to) - f64::from(from)) * progress
}

/// The accent colour for a local time, ready to be handed to the palette.
pub fn accent_at(minute: u32) -> Color {
    to_color(kelvin_to_rgb(kelvin_at(minute)))
}

/// i18n key for the part of the day, used by the sidebar status block.
pub fn phase_at(minute: u32) -> &'static str {
    let hour = f64::from(minute % 1440) / 60.0;
    if (MORNING_START..MORNING_END).contains(&hour) {
        "nightlight.phase.dawn"
    } else if (MORNING_END..EVENING_START).contains(&hour) {
        "nightlight.phase.day"
    } else if (EVENING_START..EVENING_END).contains(&hour) {
        "nightlight.phase.dusk"
    } else {
        "nightlight.phase.night"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoints_match_the_design_samples() {
        assert_eq!(kelvin_to_rgb(NIGHT_KELVIN), (0xFF, 0x9B, 0x3D));
        assert_eq!(kelvin_to_rgb(DAY_KELVIN), (0xFF, 0xD5, 0xB3));
    }

    #[test]
    fn the_curve_hits_its_anchors() {
        assert_eq!(kelvin_at(0), NIGHT_KELVIN);
        assert_eq!(kelvin_at(5 * 60), NIGHT_KELVIN);
        assert_eq!(kelvin_at(8 * 60), DAY_KELVIN);
        assert_eq!(kelvin_at(12 * 60), DAY_KELVIN);
        assert_eq!(kelvin_at(17 * 60), DAY_KELVIN);
        assert_eq!(kelvin_at(21 * 60), NIGHT_KELVIN);
        assert_eq!(kelvin_at(23 * 60 + 59), NIGHT_KELVIN);
        // 19:42, the sample used throughout the design document.
        assert!(kelvin_at(19 * 60 + 42).abs_diff(2872) <= 1);
        // Wraps instead of panicking on out-of-range input.
        assert_eq!(kelvin_at(1440), kelvin_at(0));
    }

    #[test]
    fn both_ramps_are_monotonic() {
        for minute in 5 * 60..8 * 60 {
            assert!(
                kelvin_at(minute) <= kelvin_at(minute + 1),
                "morning ramp must not cool down at {minute}"
            );
        }
        for minute in 17 * 60..21 * 60 {
            assert!(
                kelvin_at(minute) >= kelvin_at(minute + 1),
                "evening ramp must not warm back up at {minute}"
            );
        }
    }

    #[test]
    fn phases_cover_the_whole_day() {
        assert_eq!(phase_at(0), "nightlight.phase.night");
        assert_eq!(phase_at(5 * 60), "nightlight.phase.dawn");
        assert_eq!(phase_at(8 * 60), "nightlight.phase.day");
        assert_eq!(phase_at(17 * 60), "nightlight.phase.dusk");
        assert_eq!(phase_at(21 * 60), "nightlight.phase.night");
    }

    #[test]
    fn the_evening_accent_is_warmer_than_noon() {
        let noon = accent_at(12 * 60);
        let evening = accent_at(21 * 60);
        let (Color::Rgb(nr, ng, _), Color::Rgb(er, eg, _)) = (noon, evening) else {
            panic!("the accent must be an RGB colour");
        };
        // Warmer means less green and blue relative to red.
        assert!(er <= nr && eg < ng);
    }
}
