//! Unit conversions between the internal metric representation and display units.

pub const KG_PER_LB: f64 = 0.453_592_37;
pub const KJ_PER_KCAL: f64 = 4.184;
pub const CM_PER_INCH: f64 = 2.54;
pub const GRAMS_PER_OUNCE: f64 = 28.349_523_125;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum WeightUnit {
    #[default]
    Kilograms,
    Pounds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EnergyUnit {
    #[default]
    Kilocalories,
    Kilojoules,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeightUnit {
    #[default]
    Centimeters,
    FeetInches,
}

impl WeightUnit {
    pub fn symbol(self) -> &'static str {
        match self {
            WeightUnit::Kilograms => "kg",
            WeightUnit::Pounds => "lb",
        }
    }

    pub fn from_kg(self, kg: f64) -> f64 {
        match self {
            WeightUnit::Kilograms => kg,
            WeightUnit::Pounds => kg / KG_PER_LB,
        }
    }

    pub fn to_kg(self, value: f64) -> f64 {
        match self {
            WeightUnit::Kilograms => value,
            WeightUnit::Pounds => value * KG_PER_LB,
        }
    }
}

impl EnergyUnit {
    pub fn symbol(self) -> &'static str {
        match self {
            EnergyUnit::Kilocalories => "kcal",
            EnergyUnit::Kilojoules => "kJ",
        }
    }

    pub fn from_kcal(self, kcal: f64) -> f64 {
        match self {
            EnergyUnit::Kilocalories => kcal,
            EnergyUnit::Kilojoules => kcal * KJ_PER_KCAL,
        }
    }

    pub fn to_kcal(self, value: f64) -> f64 {
        match self {
            EnergyUnit::Kilocalories => value,
            EnergyUnit::Kilojoules => value / KJ_PER_KCAL,
        }
    }
}

/// Converts feet and inches to centimetres.
pub fn feet_inches_to_cm(feet: f64, inches: f64) -> f64 {
    (feet * 12.0 + inches) * CM_PER_INCH
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weight_round_trip() {
        let kg = 68.4;
        let lb = WeightUnit::Pounds.from_kg(kg);
        assert!((lb - 150.796).abs() < 0.01);
        assert!((WeightUnit::Pounds.to_kg(lb) - kg).abs() < 1e-9);
    }

    #[test]
    fn energy_round_trip() {
        assert!((EnergyUnit::Kilojoules.from_kcal(100.0) - 418.4).abs() < 1e-9);
        assert!((EnergyUnit::Kilojoules.to_kcal(418.4) - 100.0).abs() < 1e-9);
    }

    #[test]
    fn height() {
        assert!((feet_inches_to_cm(5.0, 10.0) - 177.8).abs() < 1e-9);
    }
}
