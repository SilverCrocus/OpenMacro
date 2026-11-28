//! Choosing what to order when eating out: meal-sized mains that fit what's
//! left of the day's calories, most protein per calorie first.

/// The most one meal is planned at, as a share of the day's target, so that
/// a whole day's calories left doesn't make the biggest order the best.
pub const MEAL_SHARE: f64 = 0.4;

/// Mains under this share of the meal's budget are more a snack than a
/// meal, so they're suggested only when nothing bigger fits.
pub const SNACK_SHARE: f64 = 0.25;

/// A menu item as the suggestions see it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuChoice {
    pub kcal: f64,
    pub protein_g: f64,
    /// A main, such as a burger, wrap or bowl, rather than a side, drink,
    /// dessert or sauce. Only mains are suggested.
    pub main: bool,
}

/// Suggested items, as indexes into the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Suggestions {
    /// Mains that fit the calories left, most protein per calorie first.
    Fits(Vec<usize>),
    /// No main fits, so the lightest ones, lightest first.
    Lightest(Vec<usize>),
}

impl Suggestions {
    pub fn indexes(&self) -> &[usize] {
        match self {
            Suggestions::Fits(i) | Suggestions::Lightest(i) => i,
        }
    }
}

/// Up to `count` mains from `menu` for someone with `kcal_left` calories
/// left today (zero or less when they're at or over their target) out of
/// `day_target`.
///
/// The meal's budget is what's left, but at most [`MEAL_SHARE`] of the day.
/// Mains within it and not under [`SNACK_SHARE`] of it come first; failing
/// those, any main that fits what's left. Protein per calorie is the
/// ranking because it keeps a meal filling and protein on track within a
/// calorie budget, and it's easy to explain beside each suggestion.
pub fn suggest(menu: &[MenuChoice], kcal_left: f64, day_target: f64, count: usize) -> Suggestions {
    let mains: Vec<usize> = (0..menu.len())
        .filter(|&i| menu[i].main && menu[i].kcal > 0.0)
        .collect();
    let meal = kcal_left.min(day_target * MEAL_SHARE);
    let within = |low: f64, high: f64| -> Vec<usize> {
        mains
            .iter()
            .copied()
            .filter(|&i| menu[i].kcal >= low && menu[i].kcal <= high)
            .collect()
    };
    let mut fits = within(meal * SNACK_SHARE, meal);
    if fits.is_empty() {
        fits = within(0.0, kcal_left);
    }
    if fits.is_empty() {
        let mut lightest = mains;
        lightest.sort_by(|&a, &b| menu[a].kcal.total_cmp(&menu[b].kcal));
        lightest.truncate(count);
        return Suggestions::Lightest(lightest);
    }
    let density = |i: usize| menu[i].protein_g / menu[i].kcal;
    // Ties (and equal density) go to the bigger meal, then menu order.
    fits.sort_by(|&a, &b| {
        density(b)
            .total_cmp(&density(a))
            .then(menu[b].kcal.total_cmp(&menu[a].kcal))
            .then(a.cmp(&b))
    });
    fits.truncate(count);
    Suggestions::Fits(fits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn main(kcal: f64, protein_g: f64) -> MenuChoice {
        MenuChoice {
            kcal,
            protein_g,
            main: true,
        }
    }

    fn side(kcal: f64, protein_g: f64) -> MenuChoice {
        MenuChoice {
            kcal,
            protein_g,
            main: false,
        }
    }

    #[test]
    fn ranks_fitting_mains_by_protein_per_calorie() {
        let menu = [
            main(600.0, 30.0), // 5.0 g per 100 kcal
            main(400.0, 32.0), // 8.0
            main(900.0, 60.0), // 6.7, but doesn't fit
            side(150.0, 20.0), // dense, but a side
            main(500.0, 35.0), // 7.0
        ];
        assert_eq!(
            suggest(&menu, 700.0, 2500.0, 3),
            Suggestions::Fits(vec![1, 4, 0])
        );
        assert_eq!(suggest(&menu, 700.0, 2500.0, 1), Suggestions::Fits(vec![1]));
        assert_eq!(
            suggest(&menu, 1000.0, 2500.0, 2),
            Suggestions::Fits(vec![1, 4])
        );
    }

    #[test]
    fn equal_density_prefers_the_bigger_meal() {
        let menu = [main(300.0, 15.0), main(600.0, 30.0)];
        assert_eq!(
            suggest(&menu, 800.0, 2500.0, 2),
            Suggestions::Fits(vec![1, 0])
        );
    }

    #[test]
    fn offers_the_lightest_mains_when_none_fit() {
        let menu = [
            main(700.0, 30.0),
            side(100.0, 2.0),
            main(350.0, 12.0),
            main(500.0, 40.0),
        ];
        assert_eq!(
            suggest(&menu, 300.0, 2500.0, 2),
            Suggestions::Lightest(vec![2, 3])
        );
        assert_eq!(
            suggest(&menu, -200.0, 2500.0, 5),
            Suggestions::Lightest(vec![2, 3, 0])
        );
    }

    #[test]
    fn a_menu_without_mains_has_no_suggestions() {
        let menu = [side(200.0, 3.0), main(0.0, 0.0)];
        assert_eq!(
            suggest(&menu, 1000.0, 2500.0, 3),
            Suggestions::Lightest(vec![])
        );
        assert!(suggest(&[], 1000.0, 2500.0, 3).indexes().is_empty());
    }

    #[test]
    fn a_whole_day_left_suggests_a_meal_not_a_snack() {
        let menu = [
            main(190.0, 16.0),  // a taco: the densest, but a snack
            main(770.0, 48.0),  // a burrito
            main(1110.0, 52.0), // nachos: over 40% of the day
            main(660.0, 44.0),  // a bowl
        ];
        // The meal's budget is 40% of 2600 = 1040 kcal, from 260 kcal up.
        assert_eq!(
            suggest(&menu, 2600.0, 2600.0, 3),
            Suggestions::Fits(vec![3, 1])
        );
        // With 400 kcal left the burrito and bowl don't fit; the taco does.
        assert_eq!(suggest(&menu, 400.0, 2600.0, 3), Suggestions::Fits(vec![0]));
    }

    #[test]
    fn falls_back_to_anything_that_fits_when_no_main_is_meal_sized() {
        // 40% of 2000 is 800, and the only main that fits what's left is
        // under a quarter of that.
        let menu = [main(150.0, 9.0), main(1500.0, 60.0)];
        assert_eq!(
            suggest(&menu, 2000.0, 2000.0, 3),
            Suggestions::Fits(vec![0, 1])
        );
    }
}
