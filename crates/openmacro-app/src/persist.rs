//! Saving the user's data on the device.
//!
//! Native builds (Android, desktop) write to a SQLite database in the app's
//! private data folder through `openmacro-store`. The web preview has no
//! SQLite, so it keeps everything in memory and every method is a no-op.

use chrono::NaiveDate;
use openmacro_core::expenditure::DayStatus;
use openmacro_core::log::{Goal, LogEntry, Profile};
use openmacro_core::targets::CoachedProgram;
use openmacro_core::trend::WeighIn;

use crate::state::Data;

#[cfg(not(target_arch = "wasm32"))]
pub use native::Persistence;
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

    #[cfg(target_os = "android")]
    fn data_dir() -> Option<PathBuf> {
        use jni::objects::{JObject, JString};

        // The app's private files directory, from Context.getFilesDir().
        let ctx = ndk_context::android_context();
        // SAFETY: ndk-context holds a valid JavaVM pointer and a global
        // reference to the Android context for the life of the process.
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
        let mut env = vm.attach_current_thread().ok()?;
        let context = unsafe { JObject::from_raw(ctx.context().cast()) };
        let dir = env
            .call_method(&context, "getFilesDir", "()Ljava/io/File;", &[])
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

    #[cfg(not(target_os = "android"))]
    fn data_dir() -> Option<PathBuf> {
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

        pub fn delete_entry(&self, _id: i64) {}

        pub fn insert_weigh_in(&self, _weigh_in: &WeighIn) {}

        pub fn set_day_status(&self, _date: NaiveDate, _status: DayStatus) {}

        pub fn save_settings(&self, _profile: &Profile, _goal: &Goal, _program: &CoachedProgram) {}

        pub fn replace_history(&mut self, _data: &Data) -> Option<Vec<LogEntry>> {
            None
        }
    }
}
