//! AI food logging in the app: the user's chosen service and key, asking it
//! about a photo or a description, and turning its answer into plate items.
//! The request and reply formats are in the openmacro-ai crate.
//!
//! Nothing is sent until the user logs with AI, and only to the service
//! they chose, with their own key. The key is kept out of backups.

use dioxus::prelude::*;
use openmacro_ai::{Estimate, Item, ModelChoice, Photo, Provider, Request};
use openmacro_core::food::{Amount, Food, FoodSource, Serving};
use openmacro_core::nutrients::NutrientProfile;
use serde_json::Value;

use crate::state::{AppState, PlateItem};

/// The service and model, saved with the other settings. The key is saved
/// separately (see [`KEY`]).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(
    not(target_arch = "wasm32"),
    derive(serde::Serialize, serde::Deserialize),
    serde(default)
)]
pub struct AiSettings {
    pub provider: Provider,
    pub model: String,
    /// The API's address, for an OpenAI-compatible service.
    pub base_url: String,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            provider: Provider::Gemini,
            model: Provider::Gemini.default_model().to_string(),
            base_url: String::new(),
        }
    }
}

pub static SETTINGS: GlobalSignal<AiSettings> = Signal::global(AiSettings::default);
/// The API key, if the user has saved one.
pub static KEY: GlobalSignal<Option<String>> = Signal::global(|| None);

/// Whether AI logging can be used: a model, a key (optional for an
/// OpenAI-compatible service, such as a local one), and its address.
pub fn ready() -> bool {
    let settings = SETTINGS.read();
    if settings.model.trim().is_empty() {
        return false;
    }
    match settings.provider {
        Provider::Custom => settings.base_url.trim().starts_with("http"),
        _ => KEY.read().is_some(),
    }
}

/// Loads the settings and key, and saves the settings when they change.
pub fn use_ai_settings(state: AppState) {
    use_hook(move || {
        let persist = state.persist.peek();
        if let Some(saved) = persist.load_ai_settings() {
            *SETTINGS.write() = saved;
        }
        *KEY.write() = persist.load_ai_key();
    });
    use_effect(move || {
        let settings = SETTINGS.read().clone();
        state.persist.peek().save_ai_settings(&settings);
    });
}

/// Saves or removes the key.
pub fn set_key(state: AppState, key: Option<String>) {
    let key = key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty());
    state.persist.peek().save_ai_key(key.as_deref());
    *KEY.write() = key;
}

/// Asks the chosen model about a JPEG photo, a description, or both.
pub async fn estimate(photo: Option<Vec<u8>>, note: String) -> Result<Estimate, String> {
    let settings = SETTINGS.peek().clone();
    let key = KEY.peek().clone().unwrap_or_default();
    let choice = ModelChoice::new(settings.provider, &settings.model, Some(&settings.base_url));
    let photo = photo.as_deref().map(|bytes| Photo {
        bytes,
        mime: "image/jpeg",
    });
    let request = openmacro_ai::request(&choice, &key, photo, Some(&note));
    let (status, text) = send(&request).await?;
    let service = settings.provider.label();
    let reply: Value = serde_json::from_str(&text).map_err(|_| {
        if status == 200 {
            format!("{service} sent a reply OpenMacro couldn't read.")
        } else {
            format!("{service} returned an error (HTTP {status}).")
        }
    })?;
    if status != 200 {
        return Err(failure(
            status,
            openmacro_ai::error_message(&reply),
            &choice,
        ));
    }
    openmacro_ai::parse(choice.provider, &reply)
}

/// The meal's name: the model's, or else its first foods'.
pub fn meal_name(estimate: &Estimate) -> String {
    if let Some(meal) = &estimate.meal {
        return sentence_case(meal);
    }
    let names: Vec<String> = estimate
        .items
        .iter()
        .map(|i| i.name.trim().to_lowercase())
        .collect();
    let text = match names.as_slice() {
        [] => "meal".to_string(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [a, b, rest @ ..] => format!("{a}, {b} and {} more", rest.len()),
    };
    sentence_case(&text)
}

/// A failed request in words, with what to check.
fn failure(status: u16, message: Option<String>, choice: &ModelChoice) -> String {
    let detail = message.map(|m| format!(" ({m})")).unwrap_or_default();
    match status {
        401 | 403 => format!("The API key was refused{detail}. Check it in More > AI logging."),
        404 => format!(
            "The model \"{}\" wasn't found{detail}. Check its name in More > AI logging.",
            choice.model
        ),
        402 | 429 => format!("The service is busy, or the account is out of credit{detail}."),
        _ => format!(
            "{} returned an error (HTTP {status}){detail}.",
            choice.provider.label()
        ),
    }
}

/// Sends `request` and returns the HTTP status and body. On Android the app
/// sends it (services needn't allow cross-site calls); elsewhere the web
/// view does.
#[cfg(target_os = "android")]
async fn send(request: &Request) -> Result<(u16, String), String> {
    use jni::objects::{JString, JValue};

    use crate::android::{app_class, off_thread, with_activity};

    let url = request.url.clone();
    let headers: serde_json::Map<String, Value> = request
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
        .collect();
    let headers = Value::Object(headers).to_string();
    let body = request.body.to_string();
    let reply = off_thread(move || {
        with_activity(|env, activity| {
            let class = app_class(env, activity, "dev.dioxus.main.HttpBridge")?;
            let args = [url.as_str(), headers.as_str(), body.as_str()]
                .into_iter()
                .map(|s| env.new_string(s))
                .collect::<Result<Vec<_>, _>>()?;
            let reply = env
                .call_static_method(
                    &class,
                    "post",
                    "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                    &[
                        JValue::Object(&args[0]),
                        JValue::Object(&args[1]),
                        JValue::Object(&args[2]),
                    ],
                )?
                .l()?;
            let text: String = env.get_string(&JString::from(reply))?.into();
            Ok(text)
        })
    })
    .await
    .flatten()
    .ok_or("The request couldn't be sent.")?;
    let reply: Value = serde_json::from_str(&reply).map_err(|e| e.to_string())?;
    if let Some(error) = reply["error"].as_str() {
        return Err(if error.contains("Cleartext") {
            "Plain http:// addresses only work for this phone itself (127.0.0.1). Use https://."
                .to_string()
        } else {
            "Couldn't reach the AI service. Check your connection, and the address if you set one."
                .to_string()
        });
    }
    Ok((
        reply["status"].as_u64().unwrap_or(0) as u16,
        reply["body"].as_str().unwrap_or_default().to_string(),
    ))
}

#[cfg(not(target_os = "android"))]
async fn send(request: &Request) -> Result<(u16, String), String> {
    const FETCH_JS: &str = r#"
        const req = await dioxus.recv();
        try {
            const r = await fetch(req.url, {
                method: "POST",
                headers: Object.assign({ "content-type": "application/json" }, req.headers),
                body: req.body,
            });
            return { status: r.status, body: await r.text() };
        } catch (e) {
            return { status: 0, body: String(e) };
        }
    "#;
    let headers: serde_json::Map<String, Value> = request
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
        .collect();
    let eval = document::eval(FETCH_JS);
    eval.send(serde_json::json!({
        "url": request.url,
        "headers": headers,
        "body": request.body.to_string(),
    }))
    .map_err(|e| e.to_string())?;
    let reply: Value = eval.join().await.map_err(|e| e.to_string())?;
    match reply["status"].as_u64() {
        Some(0) | None => Err(format!(
            "Couldn't reach the AI service ({}). Check your connection.",
            reply["body"].as_str().unwrap_or("no reply")
        )),
        Some(status) => Ok((
            status as u16,
            reply["body"].as_str().unwrap_or_default().to_string(),
        )),
    }
}

/// "grilled chicken breast" → "Grilled chicken breast".
fn sentence_case(name: &str) -> String {
    let name = name.trim();
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A food for one of the model's items, using its estimate of the
/// nutrients. Its id stays the same for the same name, so a food logged
/// this way shows once in recent foods.
pub fn food(item: &Item) -> Food {
    let name = sentence_case(&item.name);
    let grams = item.grams.max(1.0);
    let per_100g = NutrientProfile::from_macros(
        item.kcal.max(0.0),
        item.protein_g.max(0.0),
        item.fat_g.max(0.0),
        item.carbs_g.max(0.0),
    )
    .scaled(100.0 / grams);
    Food {
        id: format!("ai:{}", name.to_lowercase()),
        name,
        brand: None,
        source: FoodSource::Common,
        barcode: None,
        per_100g,
        servings: vec![Serving::new("portion", grams.round())],
    }
}

/// The model's items as plate items, by weight.
pub fn plate_items(items: &[Item]) -> Vec<PlateItem> {
    items
        .iter()
        .filter_map(|item| crate::foods::plate_item(&food(item), Amount::Grams(item.grams.round())))
        .collect()
}

#[cfg(test)]
mod tests {
    use openmacro_core::nutrients::Nutrient;

    use super::*;

    #[test]
    fn meals_are_named_by_the_model_or_their_foods() {
        let item = |name: &str| Item {
            name: name.into(),
            grams: 100.0,
            kcal: 100.0,
            protein_g: 0.0,
            fat_g: 0.0,
            carbs_g: 0.0,
        };
        let mut estimate = Estimate {
            meal: Some("beef curry with rice".into()),
            items: vec![
                item("White rice"),
                item("beef curry"),
                item("dal"),
                item("papadum"),
            ],
            ..Default::default()
        };
        assert_eq!(meal_name(&estimate), "Beef curry with rice");
        estimate.meal = None;
        assert_eq!(meal_name(&estimate), "White rice, beef curry and 2 more");
    }

    #[test]
    fn items_become_foods_by_weight() {
        let item = Item {
            name: "grilled chicken breast".into(),
            grams: 150.4,
            kcal: 248.0,
            protein_g: 46.0,
            fat_g: 5.4,
            carbs_g: -1.0,
        };
        let food = food(&item);
        assert_eq!(food.id, "ai:grilled chicken breast");
        assert_eq!(food.name, "Grilled chicken breast");
        assert_eq!(food.per_100g.get(Nutrient::Carbs), Some(0.0));
        let plate = plate_items(&[item]);
        assert_eq!(plate[0].amount, "150 g");
        assert!((plate[0].nutrients.get_or_zero(Nutrient::Energy) - 247.3).abs() < 0.5);
        assert_eq!(
            plate[0].food_id.as_deref(),
            Some("ai:grilled chicken breast")
        );
    }
}
