//! Saving the user's data on the device.
//!
//! Native builds (Android, desktop) write to a SQLite database in the app's
//! private data folder through `openmacro-store`. The web preview has no
//! SQLite, so it keeps everything in memory and every method is a no-op.

use chrono::NaiveDate;
use openmacro_core::checkin::CheckIn;
use openmacro_core::expenditure::DayStatus;
use openmacro_core::food::Food;
use openmacro_core::log::{Goal, LogEntry, Profile};
use openmacro_core::recipe::Recipe;
use openmacro_core::targets::{CoachedProgram, MacroTargets};
use openmacro_core::trend::WeighIn;

use crate::state::Data;

#[cfg(not(target_arch = "wasm32"))]
pub use native::Persistence;
#[cfg(all(not(target_arch = "wasm32"), feature = "bundled-foods"))]
pub use native::{data_dir, no_backup_dir};
#[cfg(target_arch = "wasm32")]
pub use web::Persistence;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::path::PathBuf;

    use dioxus::logger::tracing::warn;
    use openmacro_store::Store;

    use super::*;

    const PROFILE: &str = "profile";
    const GOAL: &str = "goal";
    const PROGRAM: &str = "program";
    const REGION: &str = "food_region";
    const CHECK_IN_DAY: &str = "check_in_day";
    const MANUAL_TARGETS: &str = "manual_targets";

    pub struct Persistence {
        store: Option<Store>,
        path: Option<PathBuf>,
    }

    /// Logs a failed write; the in-memory state stays correct either way.
    fn report<T>(what: &str, result: openmacro_store::Result<T>) -> Option<T> {
        result.map_err(|e| warn!("could not {what}: {e}")).ok()
    }

    impl Persistence {
        pub fn open() -> Self {
            let Some(dir) = data_dir() else {
                warn!("no data directory; changes will not be saved");
                return Self {
                    store: None,
                    path: None,
                };
            };
            let path = dir.join("openmacro.db");
            let store = std::fs::create_dir_all(&dir)
                .map_err(|e| warn!("could not create {}: {e}", dir.display()))
                .ok()
                .and_then(|_| report("open the database", Store::open(&path)));
            Self {
                store,
                path: Some(path),
            }
        }

        /// Where data is saved, for display.
        pub fn location(&self) -> Option<String> {
            self.store
                .as_ref()
                .and(self.path.as_ref())
                .map(|p| p.display().to_string())
        }

        /// The saved data, or `None` on first launch or if nothing could be read.
        pub fn load(&self) -> Option<Data> {
            let store = self.store.as_ref()?;
            if report("check the database", store.is_empty())? {
                return None;
            }
            let load = || -> openmacro_store::Result<Data> {
                let defaults = Data::empty(crate::state::default_profile());
                Ok(Data {
                    log: store.log_entries()?,
                    weigh_ins: store.weigh_ins()?,
                    day_status: store.day_statuses()?,
                    profile: store
                        .setting::<Profile>(PROFILE)?
                        .unwrap_or(defaults.profile),
                    goal: store.setting::<Goal>(GOAL)?.unwrap_or(defaults.goal),
                    program: store
                        .setting::<CoachedProgram>(PROGRAM)?
                        .unwrap_or(defaults.program),
                    check_ins: store.check_ins()?,
                    manual_targets: store.setting(MANUAL_TARGETS)?.flatten(),
                })
            };
            report("load saved data", load())
        }

        /// Saves a new food log entry and returns its id.
        pub fn insert_entry(&self, entry: &LogEntry) -> Option<i64> {
            report(
                "save a food entry",
                self.store.as_ref()?.insert_log_entry(entry),
            )
        }

        pub fn update_entry(&self, entry: &LogEntry) {
            if let Some(store) = &self.store {
                report("save a food entry", store.update_log_entry(entry));
            }
        }

        pub fn delete_entry(&self, id: i64) {
            if let Some(store) = &self.store {
                report("delete a food entry", store.delete_log_entry(id));
            }
        }

        pub fn insert_weigh_in(&self, weigh_in: &WeighIn) {
            if let Some(store) = &self.store {
                report("save a weigh-in", store.insert_weigh_in(weigh_in));
            }
        }

        pub fn set_day_status(&self, date: NaiveDate, status: DayStatus) {
            if let Some(store) = &self.store {
                report("save the day status", store.set_day_status(date, status));
            }
        }

        pub fn save_settings(&self, profile: &Profile, goal: &Goal, program: &CoachedProgram) {
            if let Some(store) = &self.store {
                report("save the profile", store.set_setting(PROFILE, profile));
                report("save the goal", store.set_setting(GOAL, goal));
                report("save the program", store.set_setting(PROGRAM, program));
            }
        }

        /// Saves a custom food; returns its id (`custom:<n>`).
        pub fn insert_custom_food(&self, food: &Food) -> Option<String> {
            let id = report(
                "save a custom food",
                self.store.as_ref()?.insert_custom_food(food),
            )?;
            Some(format!("custom:{id}"))
        }

        /// Saves a recipe; returns its food id (`custom:<n>`).
        pub fn insert_recipe(&self, recipe: &Recipe) -> Option<String> {
            let id = report("save a recipe", self.store.as_ref()?.insert_recipe(recipe))?;
            Some(format!("custom:{id}"))
        }

        pub fn update_custom_food(&self, food: &Food) {
            if let (Some(store), Some(n)) = (&self.store, custom_id(&food.id)) {
                report("save a custom food", store.update_custom_food(n, food));
            }
        }

        pub fn update_recipe(&self, id: &str, recipe: &Recipe) {
            if let (Some(store), Some(n)) = (&self.store, custom_id(id)) {
                report("save a recipe", store.update_recipe(n, recipe));
            }
        }

        /// Every recipe, keyed by its food id (`custom:<n>`).
        pub fn recipes(&self) -> Vec<(String, Recipe)> {
            self.store
                .as_ref()
                .and_then(|s| report("load recipes", s.recipes()))
                .unwrap_or_default()
                .into_iter()
                .map(|(n, recipe)| (format!("custom:{n}"), recipe))
                .collect()
        }

        pub fn delete_custom_food(&self, id: &str) {
            let (Some(store), Some(n)) = (
                &self.store,
                id.strip_prefix("custom:").and_then(|n| n.parse().ok()),
            ) else {
                return;
            };
            report("delete a custom food", store.delete_custom_food(n));
        }

        pub fn custom_foods(&self) -> Vec<Food> {
            self.store
                .as_ref()
                .and_then(|s| report("load custom foods", s.custom_foods()))
                .unwrap_or_default()
        }

        pub fn insert_check_in(&self, check_in: &CheckIn) {
            if let Some(store) = &self.store {
                report("save a check-in", store.insert_check_in(check_in));
            }
        }

        pub fn save_manual_targets(&self, targets: Option<MacroTargets>) {
            if let Some(store) = &self.store {
                report(
                    "save the manual targets",
                    store.set_setting(MANUAL_TARGETS, &targets),
                );
            }
        }

        pub fn save_check_in_day(&self, day: &str) {
            if let Some(store) = &self.store {
                report(
                    "save the check-in day",
                    store.set_setting(CHECK_IN_DAY, &day),
                );
            }
        }

        pub fn load_check_in_day(&self) -> Option<String> {
            report(
                "load the check-in day",
                self.store.as_ref()?.setting::<String>(CHECK_IN_DAY),
            )?
        }

        pub fn set_favorite(&self, food: &Food, favorite: bool) {
            if let Some(store) = &self.store {
                report("save a favorite", store.set_favorite(food, favorite));
            }
        }

        pub fn favorites(&self) -> Vec<Food> {
            self.store
                .as_ref()
                .and_then(|s| report("load favorites", s.favorites()))
                .unwrap_or_default()
        }

        pub fn save_region(&self, code: &str) {
            if let Some(store) = &self.store {
                report("save the food region", store.set_setting(REGION, &code));
            }
        }

        pub fn load_region(&self) -> Option<String> {
            report(
                "load the food region",
                self.store.as_ref()?.setting::<String>(REGION),
            )?
        }

        /// Everything saved, for a backup.
        pub fn snapshot(&self) -> Option<openmacro_store::Snapshot> {
            report("read data for a backup", self.store.as_ref()?.snapshot())
        }

        /// Replaces everything with a backup's contents.
        pub fn restore(&mut self, snapshot: &openmacro_store::Snapshot) -> Result<(), String> {
            let store = self
                .store
                .as_mut()
                .ok_or("There's no database on this device to restore into.")?;
            store
                .restore(snapshot)
                .map_err(|e| format!("Couldn't restore the backup: {e}"))
        }

        /// Replaces all history; returns the log with the ids it was saved under.
        pub fn replace_history(&mut self, data: &Data) -> Option<Vec<LogEntry>> {
            report(
                "replace saved history",
                self.store
                    .as_mut()?
                    .replace_history(&data.log, &data.weigh_ins, &data.day_status),
            )
        }
    }

    /// The row id in a `custom:<n>` food id.
    fn custom_id(id: &str) -> Option<i64> {
        id.strip_prefix("custom:")?.parse().ok()
    }

    /// The app's private data folder.
    #[cfg(target_os = "android")]
    pub fn data_dir() -> Option<PathBuf> {
        context_dir("getFilesDir")
    }

    /// A private folder that Android's automatic backup skips, for large
    /// files the app can recreate (the bundled food database).
    #[cfg(all(target_os = "android", feature = "bundled-foods"))]
    pub fn no_backup_dir() -> Option<PathBuf> {
        context_dir("getNoBackupFilesDir")
    }

    #[cfg(all(not(target_os = "android"), feature = "bundled-foods"))]
    pub fn no_backup_dir() -> Option<PathBuf> {
        data_dir()
    }

    /// A folder from one of the Context's `File`-returning getters.
    #[cfg(target_os = "android")]
    fn context_dir(getter: &str) -> Option<PathBuf> {
        use jni::objects::{JObject, JString};

        let ctx = ndk_context::android_context();
        // SAFETY: ndk-context holds a valid JavaVM pointer and a global
        // reference to the Android context for the life of the process.
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
        let mut env = vm.attach_current_thread().ok()?;
        let context = unsafe { JObject::from_raw(ctx.context().cast()) };
        let dir = env
            .call_method(&context, getter, "()Ljava/io/File;", &[])
            .ok()?
            .l()
            .ok()?;
        let path = env
            .call_method(&dir, "getAbsolutePath", "()Ljava/lang/String;", &[])
            .ok()?
            .l()
            .ok()?;
        let path: String = env.get_string(&JString::from(path)).ok()?.into();
        Some(PathBuf::from(path))
    }

    /// The app's private data folder.
    #[cfg(not(target_os = "android"))]
    pub fn data_dir() -> Option<PathBuf> {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .map(|dir| dir.join("openmacro"))
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::*;

    /// The web preview keeps data in memory only.
    pub struct Persistence;

    impl Persistence {
        pub fn open() -> Self {
            Self
        }

        pub fn location(&self) -> Option<String> {
            None
        }

        pub fn load(&self) -> Option<Data> {
            None
        }

        pub fn insert_entry(&self, _entry: &LogEntry) -> Option<i64> {
            None
        }

        pub fn update_entry(&self, _entry: &LogEntry) {}

        pub fn delete_entry(&self, _id: i64) {}

        pub fn insert_weigh_in(&self, _weigh_in: &WeighIn) {}

        pub fn set_day_status(&self, _date: NaiveDate, _status: DayStatus) {}

        pub fn save_settings(&self, _profile: &Profile, _goal: &Goal, _program: &CoachedProgram) {}

        pub fn replace_history(&mut self, _data: &Data) -> Option<Vec<LogEntry>> {
            None
        }

        pub fn insert_custom_food(&self, _food: &Food) -> Option<String> {
            None
        }

        pub fn insert_recipe(&self, _recipe: &Recipe) -> Option<String> {
            None
        }

        pub fn update_custom_food(&self, _food: &Food) {}

        pub fn update_recipe(&self, _id: &str, _recipe: &Recipe) {}

        pub fn recipes(&self) -> Vec<(String, Recipe)> {
            Vec::new()
        }

        pub fn delete_custom_food(&self, _id: &str) {}

        pub fn custom_foods(&self) -> Vec<Food> {
            Vec::new()
        }

        pub fn insert_check_in(&self, _check_in: &CheckIn) {}

        pub fn save_manual_targets(&self, _targets: Option<MacroTargets>) {}

        pub fn save_check_in_day(&self, _day: &str) {}

        pub fn load_check_in_day(&self) -> Option<String> {
            None
        }

        pub fn set_favorite(&self, _food: &Food, _favorite: bool) {}

        pub fn favorites(&self) -> Vec<Food> {
            Vec::new()
        }

        pub fn save_region(&self, _code: &str) {}

        pub fn load_region(&self) -> Option<String> {
            None
        }
    }
}
