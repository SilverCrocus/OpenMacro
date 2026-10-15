//! Logging with AI: the logging sheet's AI tab (a photo or a description),
//! and its settings card on the More screen.

use base64::Engine;
use dioxus::core::spawn_forever;
use dioxus::prelude::*;
use openmacro_ai::Provider;
use serde_json::Value;

use crate::Route;
use crate::ai::{self, KEY, SETTINGS};
use crate::state::AppState;

/// Runs in the web view: shows the camera in `#ai-video` and, when the
/// shutter is pressed, sends one photo as a JPEG data URL, at most 1024
/// pixels on its longer side, then stops the camera.
const CAMERA_JS: &str = r#"
const video = document.getElementById("ai-video");
let stream;
try {
    stream = await navigator.mediaDevices.getUserMedia({
        video: { facingMode: "environment", width: { ideal: 1920 }, height: { ideal: 1080 } },
        audio: false,
    });
} catch (e) {
    dioxus.send({ error: (e && e.name) || String(e) });
    return;
}
const stop = () => stream.getTracks().forEach((t) => t.stop());
const watchdog = setInterval(() => {
    if (!document.body.contains(video)) { clearInterval(watchdog); stop(); }
}, 500);
video.srcObject = stream;
try { await video.play(); } catch (e) {}
dioxus.send({ ready: true });
await new Promise((resolve) => {
    document.getElementById("ai-shutter").addEventListener("click", resolve, { once: true });
});
const scale = Math.min(1, 1024 / Math.max(video.videoWidth, video.videoHeight));
const canvas = document.createElement("canvas");
canvas.width = Math.round(video.videoWidth * scale);
canvas.height = Math.round(video.videoHeight * scale);
canvas.getContext("2d").drawImage(video, 0, 0, canvas.width, canvas.height);
clearInterval(watchdog);
stop();
dioxus.send({ photo: canvas.toDataURL("image/jpeg", 0.85) });
"#;

/// Asks for a photo from the gallery and returns it as a JPEG data URL, at
/// most 1024 pixels on its longer side, or null. The input lives outside
/// the Dioxus tree (see the backup picker in more.rs).
const PICK_PHOTO_JS: &str = r#"
const input = document.createElement("input");
input.type = "file";
input.accept = "image/*";
input.style.display = "none";
document.body.appendChild(input);
const file = await new Promise((resolve) => {
    input.addEventListener("change", () => resolve(input.files[0] || null), { once: true });
    input.addEventListener("cancel", () => resolve(null), { once: true });
    input.click();
});
input.remove();
if (!file) return null;
try {
    const bitmap = await createImageBitmap(file, { imageOrientation: "from-image" });
    const scale = Math.min(1, 1024 / Math.max(bitmap.width, bitmap.height));
    const canvas = document.createElement("canvas");
    canvas.width = Math.round(bitmap.width * scale);
    canvas.height = Math.round(bitmap.height * scale);
    canvas.getContext("2d").drawImage(bitmap, 0, 0, canvas.width, canvas.height);
    return canvas.toDataURL("image/jpeg", 0.85);
} catch (e) {
    return null;
}
"#;

/// Where the AI tab is up to.
#[derive(Debug, Clone, PartialEq)]
enum Phase {
    Camera,
    /// Waiting for the model, with the photo (a data URL) if there is one.
    Asking(Option<String>),
    /// Items added to the plate.
    Added(usize),
    Failed(String),
}

/// The JPEG bytes in a data URL.
fn jpeg_bytes(data_url: &str) -> Option<Vec<u8>> {
    let data = data_url.split_once(",")?.1;
    base64::engine::general_purpose::STANDARD.decode(data).ok()
}

fn camera_problem(name: &str) -> String {
    match name {
        "NotAllowedError" | "SecurityError" => {
            "Camera access was denied. Allow it for OpenMacro in your phone's settings, or choose a photo or describe the meal instead.".into()
        }
        "NotFoundError" | "OverconstrainedError" => {
            "No camera found. Choose a photo or describe the meal instead.".into()
        }
        _ => format!("The camera couldn't start ({name}). Choose a photo or describe the meal instead."),
    }
}

#[component]
pub fn AiTab() -> Element {
    let state = use_context::<AppState>();
    let mut phase = use_signal(|| Phase::Camera);
    let mut note = use_signal(String::new);
    // The last photo, to try again after a failure.
    let mut last_photo = use_signal(|| None::<String>);

    if !ai::ready() {
        return rsx! {
            div { class: "pad ai-setup",
                p { "Log a meal from a photo, or by describing it in a few words." }
                p { class: "muted small",
                    "It uses an AI service with your own API key, such as Google Gemini (which has a free tier) or OpenRouter. Set one up first."
                }
                button {
                    class: "primary",
                    onclick: move |_| {
                        state.hide_plate();
                        navigator().push(Route::More {});
                    },
                    "Set up AI logging"
                }
            }
        };
    }

    let mut ask = move |photo: Option<String>| {
        last_photo.set(photo.clone());
        phase.set(Phase::Asking(photo.clone()));
        let text = note();
        // Not tied to this tab: switching tabs while waiting still adds the
        // foods to the plate.
        spawn_forever(async move {
            let bytes = photo.as_deref().and_then(jpeg_bytes);
            match ai::estimate(bytes, text).await {
                Ok(items) if items.is_empty() => phase.set(Phase::Failed(
                    "No food found. Try another photo, or describe the meal.".into(),
                )),
                Ok(items) => {
                    let added = ai::plate_items(&items);
                    let count = added.len();
                    for item in added {
                        state.add_to_plate(item);
                    }
                    phase.set(Phase::Added(count));
                }
                Err(message) => phase.set(Phase::Failed(message)),
            }
        });
    };

    let described = !note().trim().is_empty();
    let service = SETTINGS.read().provider.label();
    rsx! {
        match phase() {
            Phase::Camera => rsx! {
                Camera { on_photo: move |photo| ask(Some(photo)) }
            },
            Phase::Asking(photo) => rsx! {
                div { class: "ai-view",
                    if let Some(photo) = photo {
                        img { src: "{photo}", alt: "Your photo" }
                    }
                }
                p { class: "muted pad", aria_live: "polite", "Working out what's there…" }
            },
            Phase::Added(count) => rsx! {
                p { class: "pad", aria_live: "polite",
                    if count == 1 { "Added 1 item to your plate." } else { "Added {count} items to your plate." }
                    " These are estimates: tap an item to correct its amount, then log."
                }
                div { class: "button-row pad",
                    button {
                        onclick: move |_| {
                            note.set(String::new());
                            phase.set(Phase::Camera);
                        },
                        "Another photo"
                    }
                }
            },
            Phase::Failed(message) => rsx! {
                p { class: "error pad", role: "alert", "{message}" }
                div { class: "button-row pad",
                    button { class: "primary", onclick: move |_| ask(last_photo()), "Try again" }
                    button { onclick: move |_| phase.set(Phase::Camera), "Start over" }
                }
            },
        }
        if phase() == Phase::Camera {
            label { class: "field ai-note",
                span { "Add details (optional), or describe the meal instead of a photo" }
                textarea {
                    rows: "2",
                    placeholder: "e.g. 2 eggs on toast with butter, or \"about 200 g of rice\"",
                    value: "{note}",
                    oninput: move |e| note.set(e.value()),
                }
            }
            div { class: "button-row",
                button {
                    onclick: move |_| {
                        spawn(async move {
                            if let Ok(Value::String(photo)) = document::eval(PICK_PHOTO_JS).await {
                                ask(Some(photo));
                            }
                        });
                    },
                    "Choose a photo"
                }
                button { disabled: !described, onclick: move |_| ask(None), "Use the description only" }
            }
            p { class: "muted small",
                "Sent to {service} to estimate the foods and amounts."
            }
        }
    }
}

/// The live camera with a shutter button.
#[component]
fn Camera(on_photo: EventHandler<String>) -> Element {
    let mut problem = use_signal(|| None::<String>);
    let mut ready = use_signal(|| false);
    use_future(move || async move {
        let mut eval = document::eval(CAMERA_JS);
        while let Ok(msg) = eval.recv::<Value>().await {
            if let Some(name) = msg["error"].as_str() {
                problem.set(Some(camera_problem(name)));
                break;
            }
            if msg["ready"].as_bool() == Some(true) {
                ready.set(true);
            }
            if let Some(photo) = msg["photo"].as_str() {
                on_photo.call(photo.to_string());
                break;
            }
        }
    });
    rsx! {
        if let Some(problem) = problem() {
            p { class: "muted pad", "{problem}" }
        } else {
            div { class: "ai-view",
                video { id: "ai-video", "autoplay": "true", "playsinline": "true", "muted": "true" }
                // The camera script listens for this button's click itself.
                button {
                    id: "ai-shutter",
                    class: "ai-shutter",
                    disabled: !ready(),
                    aria_label: "Take photo",
                }
            }
        }
    }
}

/// The More screen's AI logging settings.
#[component]
pub fn AiSettingsCard() -> Element {
    let state = use_context::<AppState>();
    let settings = SETTINGS.read().clone();
    let key = KEY.read().clone();
    let mut new_key = use_signal(String::new);
    let mut testing = use_signal(|| false);
    let mut test_result = use_signal(|| None::<Result<String, String>>);
    let custom = settings.provider == Provider::Custom;
    let recommended = settings.provider.default_model();
    let key_hint = key.as_ref().map(|k| {
        let tail: String = k
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("A key ending in {tail} is saved on this phone.")
    });

    rsx! {
        section { class: "card",
            h2 { "AI logging" }
            p { class: "muted small",
                "Log a meal from a photo or a description. OpenMacro sends it to the AI service you choose, using your own API key, only when you log this way. A photo typically costs well under a cent. The key stays on this phone and isn't included in backups."
            }
            label { class: "field",
                span { "Service" }
                select {
                    onchange: move |e| {
                        if let Some(provider) = Provider::from_code(&e.value()) {
                            let mut s = SETTINGS.write();
                            s.provider = provider;
                            s.model = provider.default_model().to_string();
                            test_result.set(None);
                        }
                    },
                    for p in Provider::ALL {
                        option { key: "{p.code()}", value: p.code(), selected: settings.provider == p, {p.label()} }
                    }
                }
            }
            if custom {
                label { class: "field",
                    span { "Address of its OpenAI-compatible API" }
                    input {
                        r#type: "url",
                        placeholder: "https://api.groq.com/openai/v1",
                        value: "{settings.base_url}",
                        onchange: move |e| SETTINGS.write().base_url = e.value().trim().to_string(),
                    }
                }
            }
            label { class: "field",
                span { "Model" }
                input {
                    r#type: "text",
                    placeholder: if custom { "The service's model name" } else { recommended },
                    value: "{settings.model}",
                    onchange: move |e| SETTINGS.write().model = e.value().trim().to_string(),
                }
            }
            if !custom && settings.model != recommended {
                button {
                    class: "link-button",
                    onclick: move |_| {
                        let mut s = SETTINGS.write();
                        s.model = s.provider.default_model().to_string();
                    },
                    "Use the recommended model ({recommended})"
                }
            }
            if let Some(hint) = key_hint {
                p { class: "small", "{hint}" }
                div { class: "button-row",
                    button { onclick: move |_| ai::set_key(state, None), "Remove key" }
                }
            } else {
                label { class: "field",
                    span {
                        if custom { "API key (if the service needs one)" } else { "API key" }
                    }
                    input {
                        r#type: "password",
                        "autocomplete": "off",
                        placeholder: "Paste your key",
                        value: "{new_key}",
                        oninput: move |e| new_key.set(e.value()),
                    }
                }
                div { class: "button-row",
                    button {
                        class: "primary",
                        disabled: new_key().trim().is_empty(),
                        onclick: move |_| {
                            ai::set_key(state, Some(new_key()));
                            new_key.set(String::new());
                            test_result.set(None);
                        },
                        "Save key"
                    }
                    if let Some(page) = settings.provider.key_page() {
                        a { href: page, target: "_blank", "Get a key" }
                    }
                }
            }
            if ai::ready() {
                div { class: "button-row",
                    button {
                        disabled: testing(),
                        onclick: move |_| {
                            testing.set(true);
                            test_result.set(None);
                            spawn_forever(async move {
                                let result = ai::estimate(None, "one medium banana".into())
                                    .await
                                    .and_then(|items| {
                                        items
                                            .first()
                                            .map(|i| format!("It works: {}, {:.0} g, {:.0} kcal.", i.name, i.grams, i.kcal))
                                            .ok_or_else(|| "The service answered, but found no food in \"one medium banana\".".to_string())
                                    });
                                test_result.set(Some(result));
                                testing.set(false);
                            });
                        },
                        if testing() { "Testing…" } else { "Test" }
                    }
                }
            }
            match test_result() {
                Some(Ok(text)) => rsx! { p { class: "small", "{text}" } },
                Some(Err(text)) => rsx! { p { class: "error", "{text}" } },
                None => rsx! {},
            }
            if custom {
                p { class: "muted small",
                    "Works with any OpenAI-compatible API that accepts images: Groq, Mistral, DeepSeek, Together, or Ollama or LM Studio on your own computer (use https://, or http://127.0.0.1 for one running on this phone)."
                }
            }
        }
    }
}
