//! Amount entry for the logging number pad: key handling and units.

use openmacro_core::food::{Amount, Food, parse_quantity};
use openmacro_core::units::GRAMS_PER_OUNCE;

use crate::foods::{fmt_qty, plate_item};
use crate::state::PlateItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Digit(u8),
    Dot,
    Slash,
    Space,
    Backspace,
}

/// Longest amount the pad will accept, e.g. "12 15/16".
const MAX_LEN: usize = 8;

/// Applies one key press to the amount text.
///
/// `fresh` means the text is still the pre-filled default: the first digit or
/// dot replaces it rather than appending, so typing "2" gives "2", not "12".
/// Slash and space still extend it, so "1" then space, "1", "/", "2" gives "1 1/2".
pub fn apply_key(current: &str, key: Key, fresh: bool) -> String {
    let base = if fresh && matches!(key, Key::Digit(_) | Key::Dot) {
        ""
    } else {
        current
    };
    let mut text = base.to_string();
    let last_term = text.rsplit(' ').next().unwrap_or("").to_string();
    let room = text.len() < MAX_LEN;
    let ends_with_digit = text.ends_with(|c: char| c.is_ascii_digit());

    match key {
        Key::Backspace => {
            text.pop();
        }
        Key::Digit(d) if room && d <= 9 => {
            if text == "0" {
                text.clear();
            }
            text.push(char::from(b'0' + d));
        }
        Key::Dot if room && !last_term.contains(['.', '/']) => {
            if last_term.is_empty() {
                text.push('0');
            }
            text.push('.');
        }
        Key::Slash if room && ends_with_digit && !last_term.contains(['.', '/']) => text.push('/'),
        Key::Space if room && ends_with_digit && text.chars().all(|c| c.is_ascii_digit()) => {
            text.push(' ')
        }
        _ => {}
    }
    text
}

/// A unit the amount can be entered in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// One of the food's named servings, by index.
    Serving(usize),
    Grams,
    Ounces,
}

/// Every unit offered for a food: its servings, then grams and ounces.
pub fn units(food: &Food) -> Vec<Unit> {
    (0..food.servings.len())
        .map(Unit::Serving)
        .chain([Unit::Grams, Unit::Ounces])
        .collect()
}

pub fn unit_label(food: &Food, unit: Unit) -> String {
    match unit {
        Unit::Serving(i) => food
            .servings
            .get(i)
            .map_or_else(String::new, |s| s.label.clone()),
        Unit::Grams => "g".into(),
        Unit::Ounces => "oz".into(),
    }
}

/// Grams in one of `unit`.
pub fn unit_grams(food: &Food, unit: Unit) -> Option<f64> {
    match unit {
        Unit::Serving(i) => food.servings.get(i).map(|s| s.grams),
        Unit::Grams => Some(1.0),
        Unit::Ounces => Some(GRAMS_PER_OUNCE),
    }
}

/// Re-expresses the same amount in another unit, e.g. "1" container → "170" g.
pub fn convert(food: &Food, text: &str, from: Unit, to: Unit) -> Option<String> {
    let grams = parse_quantity(text)? * unit_grams(food, from)?;
    let value = grams / unit_grams(food, to)?;
    let rounded = if to == Unit::Grams {
        (value * 10.0).round() / 10.0
    } else {
        value
    };
    Some(fmt_qty(rounded))
}

/// The plate item for `text` of `unit`, or `None` if the amount doesn't parse.
pub fn item_for(food: &Food, text: &str, unit: Unit) -> Option<PlateItem> {
    let qty = parse_quantity(text)?;
    let amount = match unit {
        Unit::Serving(index) => Amount::Servings { index, count: qty },
        Unit::Grams => Amount::Grams(qty),
        Unit::Ounces => Amount::Grams(qty * GRAMS_PER_OUNCE),
    };
    let mut item = plate_item(food, amount)?;
    if unit == Unit::Ounces {
        item.amount = format!("{} oz ({} g)", fmt_qty(qty), fmt_qty(qty * GRAMS_PER_OUNCE));
    }
    Some(item)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foods::food_by_id;
    use openmacro_core::nutrients::Nutrient;

    fn press(keys: &[Key]) -> String {
        let mut text = "1".to_string();
        let mut fresh = true;
        for &k in keys {
            text = apply_key(&text, k, fresh);
            fresh = false;
        }
        text
    }

    #[test]
    fn first_digit_replaces_default() {
        assert_eq!(press(&[Key::Digit(2)]), "2");
        assert_eq!(press(&[Key::Digit(2), Key::Digit(5)]), "25");
        assert_eq!(press(&[Key::Dot, Key::Digit(5)]), "0.5");
    }

    #[test]
    fn fractions_and_mixed_numbers() {
        assert_eq!(
            press(&[Key::Space, Key::Digit(1), Key::Slash, Key::Digit(2)]),
            "1 1/2"
        );
        assert_eq!(press(&[Key::Digit(3), Key::Slash, Key::Digit(4)]), "3/4");
        // No second slash, no dot inside a fraction, no space after a fraction.
        assert_eq!(
            press(&[
                Key::Digit(3),
                Key::Slash,
                Key::Slash,
                Key::Dot,
                Key::Digit(4),
                Key::Space
            ]),
            "3/4"
        );
    }

    #[test]
    fn rejects_malformed_input() {
        assert_eq!(
            press(&[Key::Digit(1), Key::Dot, Key::Dot, Key::Digit(5)]),
            "1.5"
        );
        assert_eq!(apply_key("", Key::Slash, false), "");
        assert_eq!(apply_key("0", Key::Digit(7), false), "7");
        assert_eq!(apply_key("12345678", Key::Digit(9), false), "12345678");
        assert_eq!(apply_key("1.5", Key::Backspace, false), "1.");
    }

    #[test]
    fn unit_conversion_keeps_the_amount() {
        let yogurt = food_by_id("greek-yogurt").unwrap();
        assert_eq!(
            convert(yogurt, "1", Unit::Serving(0), Unit::Grams).as_deref(),
            Some("170")
        );
        assert_eq!(
            convert(yogurt, "85", Unit::Grams, Unit::Serving(0)).as_deref(),
            Some("0.5")
        );
        assert_eq!(
            convert(yogurt, "1", Unit::Ounces, Unit::Grams).as_deref(),
            Some("28.3")
        );
        assert_eq!(convert(yogurt, "1/", Unit::Grams, Unit::Ounces), None);
    }

    #[test]
    fn ounce_items_are_labelled_in_ounces() {
        let yogurt = food_by_id("greek-yogurt").unwrap();
        let item = item_for(yogurt, "2", Unit::Ounces).unwrap();
        assert_eq!(item.amount, "2 oz (56.7 g)");
        assert!((item.nutrients.get(Nutrient::Energy).unwrap() - 59.0 * 0.567).abs() < 0.01);
        assert_eq!(units(yogurt), [Unit::Serving(0), Unit::Grams, Unit::Ounces]);
    }
}
