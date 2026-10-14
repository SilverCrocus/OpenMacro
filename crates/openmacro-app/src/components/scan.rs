//! The Scan tab: live barcode scanning with manual entry as a fallback.

use base64::Engine;
use dioxus::prelude::*;
use openmacro_core::food::Food;
use serde_json::Value;

use super::Icon;
use crate::barcode;
use crate::food_search::{self, Database};
use crate::state::AppState;

/// Runs in the web view: streams downscaled grayscale frames to Rust, one at
/// a time, and stops the camera when told to or when the view goes away.
const CAMERA_JS: &str = r#"
const video = document.getElementById("scan-video");
let stream;
try {
    stream = await navigator.mediaDevices.getUserMedia({
        video: { facingMode: "environment", width: { ideal: 1280 }, height: { ideal: 720 } },
        audio: false,
    });
} catch (e) {
    dioxus.send({ error: (e && e.name) || String(e) });
    return;
}
const track = stream.getVideoTracks()[0];
const stop = () => stream.getTracks().forEach((t) => t.stop());
const watchdog = setInterval(() => {
    if (!document.body.contains(video)) { clearInterval(watchdog); stop(); }
}, 500);
video.srcObject = stream;
try { await video.play(); } catch (e) {}
const canTorch = !!(track.getCapabilities && track.getCapabilities().torch);
dioxus.send({ ready: true, torch: canTorch });

const canvas = document.createElement("canvas");
const ctx = canvas.getContext("2d", { willReadFrequently: true });
let running = true;
while (running && document.body.contains(video)) {
    if (video.readyState >= 2 && video.videoWidth) {
        const scale = Math.min(1, 720 / video.videoWidth);
        const w = Math.round(video.videoWidth * scale);
        const h = Math.round(video.videoHeight * scale);
        canvas.width = w;
        canvas.height = h;
        ctx.drawImage(video, 0, 0, w, h);
        const rgba = ctx.getImageData(0, 0, w, h).data;
        const luma = new Uint8Array(w * h);
        for (let i = 0, j = 0; j < luma.length; i += 4, j++) {
            luma[j] = (rgba[i] * 77 + rgba[i + 1] * 150 + rgba[i + 2] * 29) >> 8;
        }
        let binary = "";
        for (let i = 0; i < luma.length; i += 0x8000) {
            binary += String.fromCharCode.apply(null, luma.subarray(i, i + 0x8000));
        }
        dioxus.send({ w, h, data: btoa(binary) });
        // Wait for Rust to finish with this frame before taking the next.
        while (true) {
            const msg = await dioxus.recv();
            if (msg === "stop") { running = false; break; }
            if (msg === "torch-on" || msg === "torch-off") {
                try { await track.applyConstraints({ advanced: [{ torch: msg === "torch-on" }] }); } catch (e) {}
                continue;
            }
            break;
        }
    } else {
        await new Promise((r) => setTimeout(r, 100));
    }
}
clearInterval(watchdog);
stop();
"#;

#[derive(Debug, Clone, PartialEq)]
enum Status {
    Starting,
    Scanning,
    LookingUp(String),
    NotFound(String),
    Problem(String),
}

/// Finds a product for `code` in the user's foods and the database, then on
/// Open Food Facts.
async fn lookup(
    foods: Resource<Option<Database>>,
    custom: Signal<Vec<Food>>,
    code: String,
) -> Result<Option<Food>, String> {
    let local = {
        let guard = foods.read();
        food_search::by_barcode(
            guard.as_ref().and_then(Option::as_ref),
            &custom.read(),
            &code,
        )
    };
    if let Some(found) = local {
        return Ok(Some(found.food));
    }
    for variant in barcode::lookup_variants(&code) {
        if let Some(food) = barcode::fetch_off(&variant).await? {
            return Ok(Some(food));
        }
    }
    Ok(None)
}

fn camera_problem(name: &str) -> String {
    match name {
        "NotAllowedError" | "SecurityError" => {
            "Camera access was denied. Allow it for OpenMacro in your phone's settings, or type the barcode below.".into()
        }
        "NotFoundError" | "OverconstrainedError" => "No camera found. Type the barcode below.".into(),
        _ => format!("The camera couldn't start ({name}). Type the barcode below."),
    }
}

#[component]
pub fn ScanTab(on_found: EventHandler<Food>, on_create: EventHandler<String>) -> Element {
    let state = use_context::<AppState>();
    let foods = use_context::<Resource<Option<Database>>>();
    let mut status = use_signal(|| Status::Starting);
    let mut torch_available = use_signal(|| false);
    let mut torch = use_signal(|| false);
    let mut manual = use_signal(String::new);

    let mut handle_code = move |code: String| {
        status.set(Status::LookingUp(code.clone()));
        spawn(async move {
            match lookup(foods, state.custom_foods, code.clone()).await {
                Ok(Some(food)) => on_found.call(food),
                Ok(None) => status.set(Status::NotFound(code)),
                Err(message) => status.set(Status::Problem(message)),
            }
        });
    };

    use_future(move || async move {
        let mut eval = document::eval(CAMERA_JS);
        let mut torch_sent = false;
        loop {
            let Ok(msg) = eval.recv::<Value>().await else {
                break;
            };
            if let Some(name) = msg["error"].as_str() {
                status.set(Status::Problem(camera_problem(name)));
                break;
            }
            if msg["ready"].as_bool() == Some(true) {
                torch_available.set(msg["torch"].as_bool().unwrap_or(false));
                status.set(Status::Scanning);
                continue;
            }
            let (Some(w), Some(h), Some(data)) =
                (msg["w"].as_u64(), msg["h"].as_u64(), msg["data"].as_str())
            else {
                continue;
            };
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(data)
                .ok()
                .and_then(|luma| barcode::decode_luma(luma, w as u32, h as u32));
            if let Some(code) = decoded {
                let _ = eval.send("stop");
                handle_code(code);
                break;
            }
            if torch() != torch_sent {
                torch_sent = torch();
                let _ = eval.send(if torch_sent { "torch-on" } else { "torch-off" });
            }
            if eval.send("next").is_err() {
                break;
            }
        }
    });

    let code = manual();
    let valid = barcode::valid_checksum(code.trim());
    let message = match status() {
        Status::Starting => "Starting the camera…".to_string(),
        Status::Scanning => "Point the camera at a barcode.".to_string(),
        Status::LookingUp(code) => format!("Looking up {code}…"),
        Status::NotFound(code) => format!(
            "No product found for {code}. Use Quick Add for now; custom foods are coming next."
        ),
        Status::Problem(message) => message,
    };

    rsx! {
        div { class: "scan-view",
            video { id: "scan-video", "autoplay": "true", "playsinline": "true", "muted": "true" }
            div { class: "scan-guide" }
            if torch_available() {
                button {
                    class: if torch() { "icon-btn scan-torch active" } else { "icon-btn scan-torch" },
                    aria_label: if torch() { "Turn torch off" } else { "Turn torch on" },
                    onclick: move |_| torch.set(!torch()),
                    "💡"
                }
            }
        }
        p { class: "muted pad scan-message", aria_live: "polite", "{message}" }
        if let Status::NotFound(code) = status() {
            button { class: "primary wide", onclick: move |_| on_create.call(code.clone()), "Create a food with this barcode" }
        }
        div { class: "amount-row manual-barcode",
            input {
                r#type: "text",
                "inputmode": "numeric",
                placeholder: "Type a barcode",
                aria_label: "Barcode",
                // Uncontrolled: writing the value back while typing can drop keystrokes.
                oninput: move |e| manual.set(e.value().chars().filter(char::is_ascii_digit).collect()),
            }
            button {
                class: "primary",
                disabled: !valid,
                onclick: move |_| handle_code(manual().trim().to_string()),
                Icon { name: "search" }
                "Look up"
            }
        }
        if !code.is_empty() && !valid {
            p { class: "muted small", "Barcodes have 8, 12 or 13 digits with a valid check digit." }
        }
    }
}
