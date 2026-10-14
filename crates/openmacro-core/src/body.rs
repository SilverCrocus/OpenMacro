//! Body measurements: circumferences and body fat over time.

use chrono::NaiveDate;

/// What was measured. Lengths are stored in centimetres, body fat in percent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MeasurementKind {
    BodyFat,
    Neck,
    Shoulders,
    Chest,
    Waist,
    Hips,
    LeftBicep,
    RightBicep,
    LeftForearm,
    RightForearm,
    LeftThigh,
    RightThigh,
    LeftCalf,
    RightCalf,
}

impl MeasurementKind {
    pub const ALL: [MeasurementKind; 14] = [
        MeasurementKind::BodyFat,
        MeasurementKind::Neck,
        MeasurementKind::Shoulders,
        MeasurementKind::Chest,
        MeasurementKind::Waist,
        MeasurementKind::Hips,
        MeasurementKind::LeftBicep,
        MeasurementKind::RightBicep,
        MeasurementKind::LeftForearm,
        MeasurementKind::RightForearm,
        MeasurementKind::LeftThigh,
        MeasurementKind::RightThigh,
        MeasurementKind::LeftCalf,
        MeasurementKind::RightCalf,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MeasurementKind::BodyFat => "Body fat",
            MeasurementKind::Neck => "Neck",
            MeasurementKind::Shoulders => "Shoulders",
            MeasurementKind::Chest => "Chest",
            MeasurementKind::Waist => "Waist",
            MeasurementKind::Hips => "Hips",
            MeasurementKind::LeftBicep => "Left bicep",
            MeasurementKind::RightBicep => "Right bicep",
            MeasurementKind::LeftForearm => "Left forearm",
            MeasurementKind::RightForearm => "Right forearm",
            MeasurementKind::LeftThigh => "Left thigh",
            MeasurementKind::RightThigh => "Right thigh",
            MeasurementKind::LeftCalf => "Left calf",
            MeasurementKind::RightCalf => "Right calf",
        }
    }

    /// Stable text form for storage.
    pub fn key(self) -> &'static str {
        match self {
            MeasurementKind::BodyFat => "body_fat",
            MeasurementKind::Neck => "neck",
            MeasurementKind::Shoulders => "shoulders",
            MeasurementKind::Chest => "chest",
            MeasurementKind::Waist => "waist",
            MeasurementKind::Hips => "hips",
            MeasurementKind::LeftBicep => "left_bicep",
            MeasurementKind::RightBicep => "right_bicep",
            MeasurementKind::LeftForearm => "left_forearm",
            MeasurementKind::RightForearm => "right_forearm",
            MeasurementKind::LeftThigh => "left_thigh",
            MeasurementKind::RightThigh => "right_thigh",
            MeasurementKind::LeftCalf => "left_calf",
            MeasurementKind::RightCalf => "right_calf",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }

    /// Body fat is a percentage; everything else is a length.
    pub fn is_length(self) -> bool {
        self != MeasurementKind::BodyFat
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Measurement {
    pub date: NaiveDate,
    pub kind: MeasurementKind,
    /// Centimetres, or percent for body fat.
    pub value: f64,
}

/// The latest value of `kind` on or before each date it was measured,
/// oldest first. Several on one day count as their average.
pub fn series(measurements: &[Measurement], kind: MeasurementKind) -> Vec<(NaiveDate, f64)> {
    let mut days: std::collections::BTreeMap<NaiveDate, (f64, u32)> = Default::default();
    for m in measurements.iter().filter(|m| m.kind == kind) {
        let e = days.entry(m.date).or_insert((0.0, 0));
        e.0 += m.value;
        e.1 += 1;
    }
    days.into_iter()
        .map(|(d, (sum, n))| (d, sum / n as f64))
        .collect()
}

/// The most recent value of `kind`, if ever measured.
pub fn latest(measurements: &[Measurement], kind: MeasurementKind) -> Option<(NaiveDate, f64)> {
    series(measurements, kind).last().copied()
}

/// Waist divided by height: under 0.5 is the usual healthy guide.
pub fn waist_to_height(measurements: &[Measurement], height_cm: f64) -> Option<f64> {
    let (_, waist) = latest(measurements, MeasurementKind::Waist)?;
    (height_cm > 0.0).then(|| waist / height_cm)
}

/// Waist divided by hips, from the latest of each.
pub fn waist_to_hip(measurements: &[Measurement]) -> Option<f64> {
    let (_, waist) = latest(measurements, MeasurementKind::Waist)?;
    let (_, hips) = latest(measurements, MeasurementKind::Hips)?;
    (hips > 0.0).then(|| waist / hips)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(day: u32, kind: MeasurementKind, value: f64) -> Measurement {
        Measurement {
            date: NaiveDate::from_ymd_opt(2026, 10, day).unwrap(),
            kind,
            value,
        }
    }

    #[test]
    fn series_averages_a_day_and_ratios_use_the_latest() {
        let all = [
            m(1, MeasurementKind::Waist, 86.0),
            m(8, MeasurementKind::Waist, 84.0),
            m(8, MeasurementKind::Waist, 85.0),
            m(8, MeasurementKind::Hips, 100.0),
            m(9, MeasurementKind::Neck, 38.0),
        ];
        let waist = series(&all, MeasurementKind::Waist);
        assert_eq!(waist.len(), 2);
        assert_eq!(waist[1].1, 84.5);
        assert_eq!(waist_to_hip(&all), Some(0.845));
        assert!((waist_to_height(&all, 178.0).unwrap() - 0.4747).abs() < 1e-3);
        assert_eq!(latest(&all, MeasurementKind::Chest), None);
    }

    #[test]
    fn keys_round_trip() {
        for kind in MeasurementKind::ALL {
            assert_eq!(MeasurementKind::from_key(kind.key()), Some(kind));
        }
    }
}
