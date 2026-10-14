//! In-app updates from GitHub Releases.
//!
//! On launch the app asks GitHub for the latest release. If its version is
//! newer, the dashboard offers it. On Android, "Update" downloads the APK built
//! for this CPU with the system DownloadManager and opens the package
//! installer; Android asks the user once to allow installs from OpenMacro.

use serde_json::Value;

pub const REPO: &str = "SilverCrocus/OpenMacro";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub apk_url: String,
    pub apk_size: u64,
}

/// Where an update stands, for the UI. Only Android downloads and installs.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available(Release),
    /// The user needs to allow installs from OpenMacro first.
    NeedsPermission(Release),
    /// Progress between 0 and 1 when the size is known.
    Downloading(Release, Option<f64>),
    Installing(Release),
    Failed(String),
}

/// "v1.2.3" or "1.2.3" → (1, 2, 3). Pre-release suffixes are ignored.
pub fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let core = text.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let version = (
        parts.next()??,
        parts.next().flatten().unwrap_or(0),
        parts.next().flatten().unwrap_or(0),
    );
    Some(version)
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// File name ending of the APK for this device's CPU, matching what the
/// release workflow uploads.
pub fn apk_suffix() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64-v8a.apk"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64.apk"
    } else if cfg!(target_arch = "arm") {
        "armeabi-v7a.apk"
    } else {
        "universal.apk"
    }
}

/// Reads the GitHub "latest release" JSON. Drafts and pre-releases are ignored.
pub fn parse_release(json: &Value, suffix: &str) -> Option<Release> {
    if json["draft"].as_bool() == Some(true) || json["prerelease"].as_bool() == Some(true) {
        return None;
    }
    let version = json["tag_name"]
        .as_str()?
        .trim_start_matches('v')
        .to_string();
    parse_version(&version)?;
    let asset = json["assets"]
        .as_array()?
        .iter()
        .find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(suffix)))?;
    Some(Release {
        version,
        notes: json["body"].as_str().unwrap_or_default().trim().to_string(),
        apk_url: asset["browser_download_url"].as_str()?.to_string(),
        apk_size: asset["size"].as_u64().unwrap_or(0),
    })
}

/// Asks GitHub for the latest release, through the web view's fetch so no
/// separate HTTP/TLS stack is needed. `Ok(None)` means up to date (or no
/// release yet); `Err` means GitHub couldn't be reached.
pub async fn check() -> Result<Option<Release>, String> {
    let script = format!(
        r#"try {{
            const r = await fetch("https://api.github.com/repos/{REPO}/releases/latest",
                {{ headers: {{ Accept: "application/vnd.github+json" }} }});
            return {{ status: r.status, body: r.ok ? await r.json() : null }};
        }} catch (e) {{
            return {{ status: 0, body: null }};
        }}"#
    );
    let reply: Value = dioxus::prelude::document::eval(&script)
        .join()
        .await
        .map_err(|e| format!("Couldn't check for updates: {e}"))?;
    match reply["status"].as_u64() {
        Some(200) => Ok(parse_release(&reply["body"], apk_suffix())
            .filter(|r| is_newer(&r.version, CURRENT_VERSION))),
        // No published release yet.
        Some(404) => Ok(None),
        _ => Err("Couldn't reach GitHub. Check your connection and try again.".into()),
    }
}

/// Android-only download and install, through the platform APIs over JNI.
#[cfg(target_os = "android")]
pub mod android {
    use jni::JNIEnv;
    use jni::objects::{JObject, JString, JValue};

    use crate::android::with_activity;

    /// Where a download stands.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum Download {
        /// Progress between 0 and 1, when the size is known.
        Running(Option<f64>),
        Done,
        Failed,
    }

    const APK_MIME: &str = "application/vnd.android.package-archive";
    const FILE_NAME: &str = "openmacro-update.apk";

    fn uri<'a>(env: &mut JNIEnv<'a>, text: &str) -> jni::errors::Result<JObject<'a>> {
        let s = env.new_string(text)?;
        env.call_static_method(
            "android/net/Uri",
            "parse",
            "(Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&s)],
        )?
        .l()
    }

    fn download_manager<'a>(
        env: &mut JNIEnv<'a>,
        activity: &JObject,
    ) -> jni::errors::Result<JObject<'a>> {
        let name = env.new_string("download")?;
        env.call_method(
            activity,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&name)],
        )?
        .l()
    }

    fn start_activity(
        env: &mut JNIEnv,
        activity: &JObject,
        intent: &JObject,
    ) -> jni::errors::Result<()> {
        env.call_method(
            activity,
            "startActivity",
            "(Landroid/content/Intent;)V",
            &[JValue::Object(intent)],
        )?;
        Ok(())
    }

    /// Whether the user has allowed OpenMacro to install apps.
    pub fn can_install() -> bool {
        with_activity(|env, activity| {
            let pm = env
                .call_method(
                    activity,
                    "getPackageManager",
                    "()Landroid/content/pm/PackageManager;",
                    &[],
                )?
                .l()?;
            env.call_method(&pm, "canRequestPackageInstalls", "()Z", &[])?
                .z()
        })
        .unwrap_or(false)
    }

    /// Opens the system page where the user allows installs from OpenMacro.
    pub fn open_install_permission_settings() -> bool {
        with_activity(|env, activity| {
            let package: JString = env
                .call_method(activity, "getPackageName", "()Ljava/lang/String;", &[])?
                .l()?
                .into();
            let package: String = env.get_string(&package)?.into();
            let data = uri(env, &format!("package:{package}"))?;
            let action = env.new_string("android.settings.MANAGE_UNKNOWN_APP_SOURCES")?;
            let intent = env.new_object(
                "android/content/Intent",
                "(Ljava/lang/String;Landroid/net/Uri;)V",
                &[JValue::Object(&action), JValue::Object(&data)],
            )?;
            start_activity(env, activity, &intent)
        })
        .is_some()
    }

    /// Starts downloading the APK; returns the DownloadManager id.
    pub fn start_download(url: &str, version: &str) -> Option<i64> {
        with_activity(|env, activity| {
            // Remove a previous update so the new file keeps the same name.
            let downloads = env
                .get_static_field(
                    "android/os/Environment",
                    "DIRECTORY_DOWNLOADS",
                    "Ljava/lang/String;",
                )?
                .l()?;
            let dir = env
                .call_method(
                    activity,
                    "getExternalFilesDir",
                    "(Ljava/lang/String;)Ljava/io/File;",
                    &[JValue::Object(&downloads)],
                )?
                .l()?;
            if !dir.is_null() {
                let path: JString = env
                    .call_method(&dir, "getAbsolutePath", "()Ljava/lang/String;", &[])?
                    .l()?
                    .into();
                let path: String = env.get_string(&path)?.into();
                let _ = std::fs::remove_file(std::path::Path::new(&path).join(FILE_NAME));
            }

            let manager = download_manager(env, activity)?;
            let source = uri(env, url)?;
            let request = env.new_object(
                "android/app/DownloadManager$Request",
                "(Landroid/net/Uri;)V",
                &[JValue::Object(&source)],
            )?;
            let title = env.new_string(format!("OpenMacro {version}"))?;
            let mime = env.new_string(APK_MIME)?;
            let file = env.new_string(FILE_NAME)?;
            let builder = "Landroid/app/DownloadManager$Request;";
            env.call_method(
                &request,
                "setTitle",
                format!("(Ljava/lang/CharSequence;){builder}"),
                &[JValue::Object(&title)],
            )?;
            env.call_method(
                &request,
                "setMimeType",
                format!("(Ljava/lang/String;){builder}"),
                &[JValue::Object(&mime)],
            )?;
            env.call_method(
                &request,
                "setDestinationInExternalFilesDir",
                format!("(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;){builder}"),
                &[
                    JValue::Object(activity),
                    JValue::Object(&downloads),
                    JValue::Object(&file),
                ],
            )?;
            // VISIBILITY_VISIBLE_NOTIFY_COMPLETED
            env.call_method(
                &request,
                "setNotificationVisibility",
                format!("(I){builder}"),
                &[JValue::Int(1)],
            )?;
            env.call_method(
                &manager,
                "enqueue",
                "(Landroid/app/DownloadManager$Request;)J",
                &[JValue::Object(&request)],
            )?
            .j()
        })
    }

    pub fn download_state(id: i64) -> Download {
        let state = with_activity(|env, activity| {
            let manager = download_manager(env, activity)?;
            let query = env.new_object("android/app/DownloadManager$Query", "()V", &[])?;
            let ids = env.new_long_array(1)?;
            env.set_long_array_region(&ids, 0, &[id])?;
            env.call_method(
                &query,
                "setFilterById",
                "([J)Landroid/app/DownloadManager$Query;",
                &[JValue::Object(&ids)],
            )?;
            let cursor = env
                .call_method(
                    &manager,
                    "query",
                    "(Landroid/app/DownloadManager$Query;)Landroid/database/Cursor;",
                    &[JValue::Object(&query)],
                )?
                .l()?;
            if cursor.is_null() || !env.call_method(&cursor, "moveToFirst", "()Z", &[])?.z()? {
                return Ok(Download::Failed);
            }
            let column = |env: &mut JNIEnv, name: &str| -> jni::errors::Result<i32> {
                let name = env.new_string(name)?;
                env.call_method(
                    &cursor,
                    "getColumnIndex",
                    "(Ljava/lang/String;)I",
                    &[JValue::Object(&name)],
                )?
                .i()
            };
            let (status_col, done_col, total_col) = (
                column(env, "status")?,
                column(env, "bytes_so_far")?,
                column(env, "total_size")?,
            );
            let status = env
                .call_method(&cursor, "getInt", "(I)I", &[JValue::Int(status_col)])?
                .i()?;
            let done = env
                .call_method(&cursor, "getLong", "(I)J", &[JValue::Int(done_col)])?
                .j()?;
            let total = env
                .call_method(&cursor, "getLong", "(I)J", &[JValue::Int(total_col)])?
                .j()?;
            env.call_method(&cursor, "close", "()V", &[])?;
            // STATUS_SUCCESSFUL = 8, STATUS_FAILED = 16
            Ok(match status {
                8 => Download::Done,
                16 => Download::Failed,
                _ => Download::Running((total > 0).then(|| done as f64 / total as f64)),
            })
        });
        state.unwrap_or(Download::Failed)
    }

    /// Opens the package installer for a finished download.
    pub fn install(id: i64) -> bool {
        with_activity(|env, activity| {
            let manager = download_manager(env, activity)?;
            let data = env
                .call_method(
                    &manager,
                    "getUriForDownloadedFile",
                    "(J)Landroid/net/Uri;",
                    &[JValue::Long(id)],
                )?
                .l()?;
            let action = env.new_string("android.intent.action.VIEW")?;
            let intent = env.new_object(
                "android/content/Intent",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&action)],
            )?;
            let mime = env.new_string(APK_MIME)?;
            env.call_method(
                &intent,
                "setDataAndType",
                "(Landroid/net/Uri;Ljava/lang/String;)Landroid/content/Intent;",
                &[JValue::Object(&data), JValue::Object(&mime)],
            )?;
            // FLAG_GRANT_READ_URI_PERMISSION | FLAG_ACTIVITY_NEW_TASK
            env.call_method(
                &intent,
                "addFlags",
                "(I)Landroid/content/Intent;",
                &[JValue::Int(0x0000_0001 | 0x1000_0000)],
            )?;
            start_activity(env, activity, &intent)
        })
        .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.10"), Some((0, 10, 0)));
        assert_eq!(parse_version("2.0.0-beta.1"), Some((2, 0, 0)));
        assert_eq!(parse_version("latest"), None);
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
        assert!(!is_newer("garbage", "0.2.0"));
    }

    fn release_json() -> Value {
        json!({
            "tag_name": "v0.3.0",
            "draft": false,
            "prerelease": false,
            "body": "Barcode scanning\n",
            "assets": [
                { "name": "openmacro-0.3.0-x86_64.apk", "size": 10, "browser_download_url": "https://example.test/x86.apk" },
                { "name": "openmacro-0.3.0-arm64-v8a.apk", "size": 42, "browser_download_url": "https://example.test/arm.apk" }
            ]
        })
    }

    #[test]
    fn release_picks_the_asset_for_this_cpu() {
        let release = parse_release(&release_json(), "arm64-v8a.apk").unwrap();
        assert_eq!(release.version, "0.3.0");
        assert_eq!(release.notes, "Barcode scanning");
        assert_eq!(release.apk_url, "https://example.test/arm.apk");
        assert_eq!(release.apk_size, 42);
        assert!(parse_release(&release_json(), "armeabi-v7a.apk").is_none());
    }

    #[test]
    fn prereleases_and_drafts_are_ignored() {
        let mut json = release_json();
        json["prerelease"] = json!(true);
        assert!(parse_release(&json, "arm64-v8a.apk").is_none());
    }
}
