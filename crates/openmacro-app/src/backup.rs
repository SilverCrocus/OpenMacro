//! Backup files: everything the user has saved, as one JSON file that can be
//! restored on this or another device.

use chrono::{DateTime, Local, NaiveDate, Utc};
use openmacro_store::Snapshot;
use serde::{Deserialize, Serialize};

const FORMAT: &str = "openmacro-backup";
/// Bumped when the file layout changes in a way older versions can't read.
const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupFile {
    pub format: String,
    pub version: u32,
    pub app_version: String,
    pub created_at: DateTime<Utc>,
    pub data: Snapshot,
}

impl BackupFile {
    pub fn new(data: Snapshot, created_at: DateTime<Utc>) -> Self {
        Self {
            format: FORMAT.into(),
            version: VERSION,
            app_version: env!("CARGO_PKG_VERSION").into(),
            created_at,
            data,
        }
    }

    /// The day the backup was made, in the device's time zone.
    fn local_date(&self) -> NaiveDate {
        self.created_at.with_timezone(&Local).date_naive()
    }

    pub fn file_name(&self) -> String {
        format!("openmacro-backup-{}.json", self.local_date())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("backup data always serializes")
    }

    /// Reads a backup file, refusing anything that isn't one or that a
    /// newer version of the app wrote.
    pub fn parse(text: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(text)
            .map_err(|_| "That file isn't an OpenMacro backup.".to_string())?;
        if value["format"] != FORMAT {
            return Err("That file isn't an OpenMacro backup.".into());
        }
        if value["version"]
            .as_u64()
            .is_none_or(|v| v > u64::from(VERSION))
        {
            return Err(format!(
                "This backup was made by a newer version of OpenMacro ({}). Update the app, then try again.",
                value["app_version"].as_str().unwrap_or("unknown")
            ));
        }
        serde_json::from_value(value).map_err(|e| format!("The backup is damaged: {e}"))
    }

    /// What the backup holds, for confirming a restore.
    pub fn summary(&self) -> String {
        let d = &self.data;
        let count =
            |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let recipes = d.custom_foods.iter().filter(|f| f.recipe.is_some()).count();
        format!(
            "Backup from {}: {}, {}, {}, {}.",
            self.local_date().format("%-d %B %Y"),
            count(d.log.len(), "food entry", "food entries"),
            count(d.weigh_ins.len(), "weigh-in", "weigh-ins"),
            count(
                d.custom_foods.len() - recipes,
                "custom food",
                "custom foods"
            ),
            count(recipes, "recipe", "recipes"),
        )
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use openmacro_core::trend::WeighIn;

    use super::*;

    fn backup() -> BackupFile {
        let data = Snapshot {
            weigh_ins: vec![WeighIn {
                date: chrono::NaiveDate::from_ymd_opt(2026, 10, 15).unwrap(),
                kg: 70.4,
            }],
            ..Default::default()
        };
        // Midday UTC is still 15 October from UTC−12 to UTC+11.
        BackupFile::new(data, Utc.with_ymd_and_hms(2026, 10, 15, 12, 0, 0).unwrap())
    }

    #[test]
    fn round_trips_and_summarises() {
        let file = backup();
        assert_eq!(file.file_name(), "openmacro-backup-2026-10-15.json");
        let parsed = BackupFile::parse(&file.to_json()).unwrap();
        assert_eq!(parsed, file);
        assert_eq!(
            parsed.summary(),
            "Backup from 15 October 2026: 0 food entries, 1 weigh-in, 0 custom foods, 0 recipes."
        );
    }

    #[test]
    fn rejects_other_files_and_newer_versions() {
        assert!(BackupFile::parse("not json").is_err());
        assert!(BackupFile::parse(r#"{"log": []}"#).is_err());
        let mut newer: serde_json::Value = serde_json::from_str(&backup().to_json()).unwrap();
        newer["version"] = 2.into();
        newer["app_version"] = "9.0.0".into();
        let err = BackupFile::parse(&newer.to_string()).unwrap_err();
        assert!(err.contains("newer version of OpenMacro (9.0.0)"), "{err}");
    }
}
