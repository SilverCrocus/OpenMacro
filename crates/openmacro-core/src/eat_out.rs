//! Choosing what to order when eating out: meal-sized mains that fit what's
//! left of the day's calories, most protein per calorie first.

/// The most one meal is planned at, as a share of the day's target, so that
/// a whole day's calories left doesn't make the biggest order the best.
pub const MEAL_SHARE: f64 = 0.4;

/// A meal-sized order is at least this share of the day's target. Under it
/// is more a snack than a meal (a single taco, one piece of chicken).
pub const MIN_MEAL_DAY_SHARE: f64 = 0.15;

/// With less left than a full meal's budget, a meal-sized order is at least
/// this share of what's left instead.
pub const MIN_MEAL_SHARE: f64 = 0.5;

/// The most slices suggested of a pizza sold by the slice: a large pizza.
pub const MAX_SLICES: u32 = 8;

/// A menu item as the suggestions see it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuChoice {
    pub kcal: f64,
    pub protein_g: f64,
    /// A main, such as a burger, wrap or bowl, rather than a side, drink,
    /// dessert or sauce. Only mains are suggested.
    pub main: bool,
    /// Listed per slice, so a meal is several of it.
    pub by_the_slice: bool,
}

/// A suggested order: `count` of the menu's item at `index`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pick {
    pub index: usize,
    pub count: u32,
}

/// Suggested orders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Suggestions {
    /// Meal-sized orders that fit the calories left, most protein per
    /// calorie first.
    Fits(Vec<Pick>),
    /// Nothing meal-sized fits, so the lightest mains, lightest first.
    Lightest(Vec<Pick>),
}

impl Suggestions {
    pub fn picks(&self) -> &[Pick] {
        match self {
            Suggestions::Fits(p) | Suggestions::Lightest(p) => p,
        }
    }
}

/// Up to `count` orders from `menu` for someone with `kcal_left` calories
/// left today (zero or less when they're at or over their target) out of
/// `day_target`.
///
/// The meal's budget is what's left, but at most [`MEAL_SHARE`] of the day.
/// An order fits if it's within the budget and at least
/// [`MIN_MEAL_DAY_SHARE`] of the day, or [`MIN_MEAL_SHARE`] of the budget if
/// that's less. A main sold by the slice is ordered as the most slices that fit,
/// up to [`MAX_SLICES`]. Protein per calorie is the ranking because it
/// keeps a meal filling and protein on track within a calorie budget, and
/// it's easy to explain beside each suggestion. When nothing fits, the
/// lightest mains are offered instead, one of each.
pub fn suggest(menu: &[MenuChoice], kcal_left: f64, day_target: f64, count: usize) -> Suggestions {
    let mains: Vec<usize> = (0..menu.len())
        .filter(|&i| menu[i].main && menu[i].kcal > 0.0)
        .collect();
    let meal = kcal_left.min(day_target * MEAL_SHARE);
    let least = (meal * MIN_MEAL_SHARE).min(day_target * MIN_MEAL_DAY_SHARE);
    let order = |i: usize| -> Pick {
        let slices = if menu[i].by_the_slice && meal > 0.0 {
            ((meal / menu[i].kcal).floor() as u32).clamp(1, MAX_SLICES)
        } else {
            1
        };
        Pick {
            index: i,
            count: slices,
        }
    };
    let kcal = |p: &Pick| menu[p.index].kcal * f64::from(p.count);
    let mut fits: Vec<Pick> = mains
        .iter()
        .map(|&i| order(i))
        .filter(|p| kcal(p) <= meal && kcal(p) >= least)
        .collect();
    if fits.is_empty() {
        let mut lightest = mains;
        lightest.sort_by(|&a, &b| menu[a].kcal.total_cmp(&menu[b].kcal));
        lightest.truncate(count);
        return Suggestions::Lightest(
            lightest
                .into_iter()
                .map(|index| Pick { index, count: 1 })
                .collect(),
        );
    }
    let density = |p: &Pick| menu[p.index].protein_g / menu[p.index].kcal;
    // Ties (and equal density) go to the bigger meal, then menu order.
    fits.sort_by(|a, b| {
        density(b)
            .total_cmp(&density(a))
            .then(kcal(b).total_cmp(&kcal(a)))
            .then(a.index.cmp(&b.index))
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
            by_the_slice: false,
        }
    }

    fn side(kcal: f64, protein_g: f64) -> MenuChoice {
        MenuChoice {
            main: false,
            ..main(kcal, protein_g)
        }
    }

    fn slice(kcal: f64, protein_g: f64) -> MenuChoice {
        MenuChoice {
            by_the_slice: true,
            ..main(kcal, protein_g)
        }
    }

    fn one(indexes: &[usize]) -> Vec<Pick> {
        indexes
            .iter()
            .map(|&index| Pick { index, count: 1 })
            .collect()
    }

    #[test]
    fn ranks_fitting_mains_by_protein_per_calorie() {
        let menu = [
            main(600.0, 30.0), // 5.0 g per 100 kcal
            main(400.0, 32.0), // 8.0
            main(900.0, 60.0), // 6.7
            side(450.0, 40.0), // dense, but a side
            main(500.0, 35.0), // 7.0
        ];
        // A 700 kcal budget, from 350 kcal (half of it) up.
        assert_eq!(
            suggest(&menu, 700.0, 2500.0, 3),
            Suggestions::Fits(one(&[1, 4, 0]))
        );
        assert_eq!(
            suggest(&menu, 700.0, 2500.0, 1),
            Suggestions::Fits(one(&[1]))
        );
        // A 1000 kcal budget, from 375 kcal (15% of the day) up.
        assert_eq!(
            suggest(&menu, 1000.0, 2500.0, 3),
            Suggestions::Fits(one(&[1, 4, 2]))
        );
    }

    #[test]
    fn equal_density_prefers_the_bigger_meal() {
        let menu = [main(300.0, 15.0), main(600.0, 30.0)];
        assert_eq!(
            suggest(&menu, 600.0, 2500.0, 2),
            Suggestions::Fits(one(&[1, 0]))
        );
    }

    #[test]
    fn a_whole_day_left_suggests_a_meal_not_a_snack() {
        let menu = [
            main(206.0, 14.0),  // a taco: the densest, but a snack
            main(770.0, 48.0),  // a burrito
            main(1110.0, 52.0), // nachos: over 40% of the day
            main(660.0, 44.0),  // a bowl
        ];
        // 40% of 2000 is 800 kcal, from 300 (15%) up.
        assert_eq!(
            suggest(&menu, 2000.0, 2000.0, 3),
            Suggestions::Fits(one(&[3, 1]))
        );
        // 40% of 2600 is 1040 kcal, from 390 up.
        assert_eq!(
            suggest(&menu, 2600.0, 2600.0, 3),
            Suggestions::Fits(one(&[3, 1]))
        );
        // With 400 kcal left, from 200 up: the taco is a meal.
        assert_eq!(
            suggest(&menu, 400.0, 2000.0, 3),
            Suggestions::Fits(one(&[0]))
        );
    }

    #[test]
    fn offers_the_lightest_mains_when_nothing_is_meal_sized() {
        let menu = [
            main(700.0, 30.0),
            side(100.0, 2.0),
            main(350.0, 12.0),
            main(500.0, 40.0),
        ];
        assert_eq!(
            suggest(&menu, 300.0, 2500.0, 2),
            Suggestions::Lightest(one(&[2, 3]))
        );
        assert_eq!(
            suggest(&menu, -200.0, 2500.0, 5),
            Suggestions::Lightest(one(&[2, 3, 0]))
        );
        // Only a snack and a feast: neither is a meal for an 800 kcal budget.
        let menu = [main(150.0, 9.0), main(1500.0, 60.0)];
        assert_eq!(
            suggest(&menu, 2000.0, 2000.0, 3),
            Suggestions::Lightest(one(&[0, 1]))
        );
        // A zero target has no budget at all.
        assert_eq!(
            suggest(&menu, 500.0, 0.0, 1),
            Suggestions::Lightest(one(&[0]))
        );
    }

    #[test]
    fn a_menu_without_mains_has_no_suggestions() {
        let menu = [side(200.0, 3.0), main(0.0, 0.0)];
        assert_eq!(
            suggest(&menu, 1000.0, 2500.0, 3),
            Suggestions::Lightest(vec![])
        );
        assert!(suggest(&[], 1000.0, 2500.0, 3).picks().is_empty());
    }

    #[test]
    fn pizza_by_the_slice_is_ordered_as_several_slices() {
        let menu = [slice(200.0, 10.0), slice(110.0, 5.0), main(700.0, 20.0)];
        // An 800 kcal budget: 4 of the first slices, and 7 of the smaller.
        assert_eq!(
            suggest(&menu, 2000.0, 2000.0, 3),
            Suggestions::Fits(vec![
                Pick { index: 0, count: 4 },
                Pick { index: 1, count: 7 },
                Pick { index: 2, count: 1 },
            ])
        );
        // At most a large pizza, even with room for more.
        assert_eq!(
            suggest(&[slice(60.0, 3.0)], 2000.0, 2000.0, 1),
            Suggestions::Fits(vec![Pick { index: 0, count: 8 }])
        );
        // Lightest is one slice.
        assert_eq!(
            suggest(&menu, 0.0, 2000.0, 1),
            Suggestions::Lightest(vec![Pick { index: 1, count: 1 }])
        );
    }
}
