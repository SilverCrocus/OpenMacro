//! Android platform calls over JNI, shared by the updater and export.

use jni::objects::{JObject, JValue};
use jni::{JNIEnv, JavaVM};

/// Runs `f` with the JNI environment and the activity, freeing local
/// references afterwards. Java exceptions become `None`.
pub fn with_activity<T>(
    f: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<T>,
) -> Option<T> {
    let ctx = ndk_context::android_context();
    // SAFETY: ndk-context holds a valid JavaVM and a global reference to
    // the activity for the life of the process.
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.ok()?;
    let mut env = vm.attach_current_thread().ok()?;
    let activity = unsafe { JObject::from_raw(ctx.context().cast()) };
    let result = env.with_local_frame(32, |env| f(env, &activity));
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
        return None;
    }
    result.ok()
}

/// The device's API level (`Build.VERSION.SDK_INT`).
pub fn sdk_version() -> Option<i32> {
    with_activity(|env, _| {
        env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
            .i()
    })
}

/// Writes a file to the shared Downloads folder, in `folder`, through
/// MediaStore. Needs Android 10 (API 29) or later and no permission. If a
/// file with that name exists, Android picks a new name.
pub fn save_to_downloads(name: &str, mime: &str, folder: &str, bytes: &[u8]) -> Option<()> {
    let relative_path = format!("Download/{folder}");
    let saved = with_activity(|env, activity| {
        let resolver = env
            .call_method(
                activity,
                "getContentResolver",
                "()Landroid/content/ContentResolver;",
                &[],
            )?
            .l()?;
        let values = env.new_object("android/content/ContentValues", "()V", &[])?;
        for (key, value) in [
            ("_display_name", name),
            ("mime_type", mime),
            ("relative_path", relative_path.as_str()),
        ] {
            let key = env.new_string(key)?;
            let value = env.new_string(value)?;
            env.call_method(
                &values,
                "put",
                "(Ljava/lang/String;Ljava/lang/String;)V",
                &[JValue::Object(&key), JValue::Object(&value)],
            )?;
        }
        let downloads = env
            .get_static_field(
                "android/provider/MediaStore$Downloads",
                "EXTERNAL_CONTENT_URI",
                "Landroid/net/Uri;",
            )?
            .l()?;
        let uri = env
            .call_method(
                &resolver,
                "insert",
                "(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;",
                &[JValue::Object(&downloads), JValue::Object(&values)],
            )?
            .l()?;
        if uri.is_null() {
            return Ok(false);
        }
        let stream = env
            .call_method(
                &resolver,
                "openOutputStream",
                "(Landroid/net/Uri;)Ljava/io/OutputStream;",
                &[JValue::Object(&uri)],
            )?
            .l()?;
        if stream.is_null() {
            return Ok(false);
        }
        let array = env.byte_array_from_slice(bytes)?;
        env.call_method(&stream, "write", "([B)V", &[JValue::Object(&array)])?;
        env.call_method(&stream, "close", "()V", &[])?;
        Ok(true)
    })?;
    saved.then_some(())
}
