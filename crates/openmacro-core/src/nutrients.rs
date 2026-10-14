//! The tracked nutrient set and nutrient profiles.

use std::collections::BTreeMap;
use std::ops::{Add, AddAssign};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NutrientUnit {
    Kcal,
    Gram,
    Milligram,
    Microgram,
}

impl NutrientUnit {
    pub fn symbol(self) -> &'static str {
        match self {
            NutrientUnit::Kcal => "kcal",
            NutrientUnit::Gram => "g",
            NutrientUnit::Milligram => "mg",
            NutrientUnit::Microgram => "mcg",
        }
    }
}

/// Grouping used for display sections (e.g. "Fat breakdown", "Vitamins").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NutrientGroup {
    Energy,
    Macro,
    Carbohydrate,
    Fat,
    AminoAcid,
    Vitamin,
    Mineral,
    Other,
}

macro_rules! nutrients {
    ($($variant:ident => ($key:literal, $name:literal, $unit:ident, $group:ident)),+ $(,)?) => {
        /// A nutrient tracked by OpenMacro.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[cfg_attr(
            feature = "serde",
            derive(serde::Serialize, serde::Deserialize),
            serde(rename_all = "snake_case")
        )]
        pub enum Nutrient {
            $($variant),+
        }

        impl Nutrient {
            /// Every tracked nutrient, in display order.
            pub const ALL: &'static [Nutrient] = &[$(Nutrient::$variant),+];

            /// Stable snake_case identifier, used for storage and serialisation.
            pub fn key(self) -> &'static str {
                match self {
                    $(Nutrient::$variant => $key),+
                }
            }

            pub fn from_key(key: &str) -> Option<Nutrient> {
                match key {
                    $($key => Some(Nutrient::$variant),)+
                    _ => None,
                }
            }

            pub fn name(self) -> &'static str {
                match self {
                    $(Nutrient::$variant => $name),+
                }
            }

            pub fn unit(self) -> NutrientUnit {
                match self {
                    $(Nutrient::$variant => NutrientUnit::$unit),+
                }
            }

            pub fn group(self) -> NutrientGroup {
                match self {
                    $(Nutrient::$variant => NutrientGroup::$group),+
                }
            }
        }
    };
}

nutrients! {
    Energy => ("energy", "Calories", Kcal, Energy),

    Protein => ("protein", "Protein", Gram, Macro),
    Fat => ("fat", "Fat", Gram, Macro),
    Carbs => ("carbs", "Carbs", Gram, Macro),
    Alcohol => ("alcohol", "Alcohol", Gram, Macro),

    Fiber => ("fiber", "Fiber", Gram, Carbohydrate),
    Starch => ("starch", "Starch", Gram, Carbohydrate),
    Sugars => ("sugars", "Sugars", Gram, Carbohydrate),
    AddedSugars => ("added_sugars", "Added sugars", Gram, Carbohydrate),

    Monounsaturated => ("monounsaturated", "Monounsaturated", Gram, Fat),
    Polyunsaturated => ("polyunsaturated", "Polyunsaturated", Gram, Fat),
    Omega3 => ("omega3", "Omega-3", Gram, Fat),
    Omega3Ala => ("omega3_ala", "Omega-3 ALA", Gram, Fat),
    Omega3Dha => ("omega3_dha", "Omega-3 DHA", Gram, Fat),
    Omega3Epa => ("omega3_epa", "Omega-3 EPA", Gram, Fat),
    Omega6 => ("omega6", "Omega-6", Gram, Fat),
    Saturated => ("saturated", "Saturated", Gram, Fat),
    TransFat => ("trans_fat", "Trans fat", Gram, Fat),

    Cysteine => ("cysteine", "Cysteine", Gram, AminoAcid),
    Histidine => ("histidine", "Histidine", Gram, AminoAcid),
    Isoleucine => ("isoleucine", "Isoleucine", Gram, AminoAcid),
    Leucine => ("leucine", "Leucine", Gram, AminoAcid),
    Lysine => ("lysine", "Lysine", Gram, AminoAcid),
    Methionine => ("methionine", "Methionine", Gram, AminoAcid),
    Phenylalanine => ("phenylalanine", "Phenylalanine", Gram, AminoAcid),
    Threonine => ("threonine", "Threonine", Gram, AminoAcid),
    Tryptophan => ("tryptophan", "Tryptophan", Gram, AminoAcid),
    Tyrosine => ("tyrosine", "Tyrosine", Gram, AminoAcid),
    Valine => ("valine", "Valine", Gram, AminoAcid),

    VitaminB1 => ("vitamin_b1", "B1, Thiamine", Milligram, Vitamin),
    VitaminB2 => ("vitamin_b2", "B2, Riboflavin", Milligram, Vitamin),
    VitaminB3 => ("vitamin_b3", "B3, Niacin", Milligram, Vitamin),
    VitaminB5 => ("vitamin_b5", "B5, Pantothenic acid", Milligram, Vitamin),
    VitaminB6 => ("vitamin_b6", "B6, Pyridoxine", Milligram, Vitamin),
    VitaminB12 => ("vitamin_b12", "B12, Cobalamin", Microgram, Vitamin),
    Folate => ("folate", "Folate", Microgram, Vitamin),
    VitaminA => ("vitamin_a", "Vitamin A", Microgram, Vitamin),
    VitaminC => ("vitamin_c", "Vitamin C", Milligram, Vitamin),
    VitaminD => ("vitamin_d", "Vitamin D", Microgram, Vitamin),
    VitaminE => ("vitamin_e", "Vitamin E", Milligram, Vitamin),
    VitaminK => ("vitamin_k", "Vitamin K", Microgram, Vitamin),

    Calcium => ("calcium", "Calcium", Milligram, Mineral),
    Copper => ("copper", "Copper", Milligram, Mineral),
    Iron => ("iron", "Iron", Milligram, Mineral),
    Magnesium => ("magnesium", "Magnesium", Milligram, Mineral),
    Manganese => ("manganese", "Manganese", Milligram, Mineral),
    Phosphorus => ("phosphorus", "Phosphorus", Milligram, Mineral),
    Potassium => ("potassium", "Potassium", Milligram, Mineral),
    Selenium => ("selenium", "Selenium", Microgram, Mineral),
    Sodium => ("sodium", "Sodium", Milligram, Mineral),
    Zinc => ("zinc", "Zinc", Milligram, Mineral),

    Caffeine => ("caffeine", "Caffeine", Milligram, Other),
    Cholesterol => ("cholesterol", "Cholesterol", Milligram, Other),
    Choline => ("choline", "Choline", Milligram, Other),
    Water => ("water", "Water", Gram, Other),
}

/// Atwater factors in kcal per gram.
pub const KCAL_PER_G_PROTEIN: f64 = 4.0;
pub const KCAL_PER_G_CARBS: f64 = 4.0;
pub const KCAL_PER_G_FAT: f64 = 9.0;
pub const KCAL_PER_G_ALCOHOL: f64 = 7.0;

/// Nutrient amounts for some quantity of food (or a whole day).
///
/// Missing nutrients are "unknown", which is distinct from zero: food databases
/// are often incomplete, and the UI should be able to show that.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
pub struct NutrientProfile {
    amounts: BTreeMap<Nutrient, f64>,
}

impl NutrientProfile {
    pub fn new() -> Self {
        Self::default()
    }

    /// Convenience constructor for the four headline values.
    pub fn from_macros(kcal: f64, protein_g: f64, fat_g: f64, carbs_g: f64) -> Self {
        let mut p = Self::new();
        p.set(Nutrient::Energy, kcal);
        p.set(Nutrient::Protein, protein_g);
        p.set(Nutrient::Fat, fat_g);
        p.set(Nutrient::Carbs, carbs_g);
        p
    }

    pub fn with(mut self, nutrient: Nutrient, amount: f64) -> Self {
        self.set(nutrient, amount);
        self
    }

    pub fn set(&mut self, nutrient: Nutrient, amount: f64) {
        self.amounts.insert(nutrient, amount);
    }

    /// The amount, or `None` if this nutrient is unknown.
    pub fn get(&self, nutrient: Nutrient) -> Option<f64> {
        self.amounts.get(&nutrient).copied()
    }

    /// The amount, treating unknown as zero.
    pub fn get_or_zero(&self, nutrient: Nutrient) -> f64 {
        self.get(nutrient).unwrap_or(0.0)
    }

    pub fn iter(&self) -> impl Iterator<Item = (Nutrient, f64)> + '_ {
        self.amounts.iter().map(|(n, a)| (*n, *a))
    }

    /// Returns a copy with every amount multiplied by `factor`.
    pub fn scaled(&self, factor: f64) -> Self {
        Self {
            amounts: self.amounts.iter().map(|(n, a)| (*n, a * factor)).collect(),
        }
    }

    /// Carbs minus fiber, if carbs are known.
    pub fn net_carbs(&self) -> Option<f64> {
        self.get(Nutrient::Carbs)
            .map(|c| (c - self.get_or_zero(Nutrient::Fiber)).max(0.0))
    }

    /// Calories implied by the macro grams (protein, fat, carbs, alcohol).
    pub fn macro_kcal(&self) -> f64 {
        self.get_or_zero(Nutrient::Protein) * KCAL_PER_G_PROTEIN
            + self.get_or_zero(Nutrient::Fat) * KCAL_PER_G_FAT
            + self.get_or_zero(Nutrient::Carbs) * KCAL_PER_G_CARBS
            + self.get_or_zero(Nutrient::Alcohol) * KCAL_PER_G_ALCOHOL
    }
}

impl AddAssign<&NutrientProfile> for NutrientProfile {
    fn add_assign(&mut self, rhs: &NutrientProfile) {
        for (n, a) in rhs.iter() {
            *self.amounts.entry(n).or_insert(0.0) += a;
        }
    }
}

impl Add<&NutrientProfile> for NutrientProfile {
    type Output = NutrientProfile;

    fn add(mut self, rhs: &NutrientProfile) -> NutrientProfile {
        self += rhs;
        self
    }
}

impl<'a> std::iter::Sum<&'a NutrientProfile> for NutrientProfile {
    fn sum<I: Iterator<Item = &'a NutrientProfile>>(iter: I) -> Self {
        iter.fold(NutrientProfile::new(), |acc, p| acc + p)
    }
}

/// Floor / target / ceiling for one nutrient. Any bound may be absent.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NutrientGoal {
    pub floor: Option<f64>,
    pub target: Option<f64>,
    pub ceiling: Option<f64>,
}

impl NutrientGoal {
    /// Bounds must be ordered floor <= target <= ceiling where present.
    pub fn is_valid(&self) -> bool {
        let ordered = |a: Option<f64>, b: Option<f64>| match (a, b) {
            (Some(a), Some(b)) => a <= b,
            _ => true,
        };
        ordered(self.floor, self.target)
            && ordered(self.target, self.ceiling)
            && ordered(self.floor, self.ceiling)
    }

    /// Fraction of the target reached, if a target is set.
    pub fn progress(&self, amount: f64) -> Option<f64> {
        self.target.filter(|t| *t > 0.0).map(|t| amount / t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nutrient_table_is_complete() {
        assert_eq!(Nutrient::ALL.len(), 55);
        assert_eq!(Nutrient::VitaminB12.unit(), NutrientUnit::Microgram);
        assert_eq!(Nutrient::Leucine.group(), NutrientGroup::AminoAcid);
    }

    #[test]
    fn keys_round_trip() {
        for &n in Nutrient::ALL {
            assert_eq!(Nutrient::from_key(n.key()), Some(n));
        }
        assert_eq!(Nutrient::VitaminB12.key(), "vitamin_b12");
        assert_eq!(Nutrient::Omega3Ala.key(), "omega3_ala");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn keys_match_serde_names() {
        for &n in Nutrient::ALL {
            assert_eq!(
                serde_json::to_string(&n).unwrap(),
                format!("\"{}\"", n.key())
            );
        }
    }

    #[test]
    fn profiles_add_and_scale() {
        let a = NutrientProfile::from_macros(100.0, 10.0, 2.0, 10.0).with(Nutrient::Fiber, 3.0);
        let b = NutrientProfile::from_macros(50.0, 0.0, 5.0, 1.0);
        let total: NutrientProfile = [a.clone(), b].iter().sum();
        assert_eq!(total.get(Nutrient::Energy), Some(150.0));
        assert_eq!(total.get(Nutrient::Fiber), Some(3.0));
        assert_eq!(total.get(Nutrient::Sodium), None);
        assert_eq!(a.scaled(2.0).get(Nutrient::Protein), Some(20.0));
        assert_eq!(a.net_carbs(), Some(7.0));
    }

    #[test]
    fn macro_kcal_uses_atwater_factors() {
        let p = NutrientProfile::from_macros(0.0, 10.0, 10.0, 10.0).with(Nutrient::Alcohol, 10.0);
        assert_eq!(p.macro_kcal(), 40.0 + 90.0 + 40.0 + 70.0);
    }

    #[test]
    fn goal_validation() {
        let ok = NutrientGoal {
            floor: Some(10.0),
            target: Some(20.0),
            ceiling: Some(40.0),
        };
        let bad = NutrientGoal {
            floor: Some(30.0),
            target: Some(20.0),
            ceiling: None,
        };
        assert!(ok.is_valid());
        assert!(!bad.is_valid());
        assert_eq!(ok.progress(10.0), Some(0.5));
    }
}
