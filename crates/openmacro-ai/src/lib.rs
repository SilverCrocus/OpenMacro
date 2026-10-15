//! AI food logging: asks a vision model what's in a photo, then matches its
//! answer to real foods.
//!
//! The model names each food and estimates its weight (and, as a fallback,
//! its energy and macros). Nutrients normally come from the food database
//! instead: models are much better at recognising food than at knowing its
//! nutrient values, and the user can correct a matched food.
//!
//! This crate builds each provider's request and parses its reply; it does
//! no networking itself (the app sends requests through its web view, and
//! the eval tool through `ureq`).

#[cfg(feature = "foods")]
pub mod matching;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// A model API OpenMacro can call with the user's own key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    Gemini,
    Anthropic,
    OpenAi,
    /// One key for many providers' models, billed in one place.
    OpenRouter,
    /// Any service with an OpenAI-compatible API at a given address: Groq,
    /// Mistral, DeepSeek, Together, or Ollama or LM Studio on your own
    /// computer.
    Custom,
}

impl Provider {
    pub const ALL: [Provider; 5] = [
        Provider::Gemini,
        Provider::OpenRouter,
        Provider::OpenAi,
        Provider::Anthropic,
        Provider::Custom,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Provider::Gemini => "gemini",
            Provider::Anthropic => "anthropic",
            Provider::OpenAi => "openai",
            Provider::OpenRouter => "openrouter",
            Provider::Custom => "custom",
        }
    }

    pub fn from_code(code: &str) -> Option<Provider> {
        Self::ALL.into_iter().find(|p| p.code() == code)
    }

    pub fn label(self) -> &'static str {
        match self {
            Provider::Gemini => "Google Gemini",
            Provider::Anthropic => "Anthropic Claude",
            Provider::OpenAi => "OpenAI",
            Provider::OpenRouter => "OpenRouter",
            Provider::Custom => "Other (OpenAI-compatible)",
        }
    }

    /// The usual environment variable for the provider's API key.
    pub fn key_variable(self) -> &'static str {
        match self {
            Provider::Gemini => "GEMINI_API_KEY",
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            Provider::OpenAi => "OPENAI_API_KEY",
            Provider::OpenRouter => "OPENROUTER_API_KEY",
            Provider::Custom => "CUSTOM_API_KEY",
        }
    }

    /// A good model to start with: small, quick and cheap. Gemini 3.1 Flash
    /// Lite did best in our comparison (docs/AI_PHOTO_EVAL.md).
    pub fn default_model(self) -> &'static str {
        self.models().first().map_or("", |(id, _)| id)
    }

    /// Small models to choose from, as (model name, description), the
    /// recommended one first. All did well in docs/AI_PHOTO_EVAL.md; any
    /// other model can still be named.
    pub fn models(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Provider::Gemini => &[
                (
                    "gemini-3.1-flash-lite",
                    "Gemini 3.1 Flash Lite (recommended)",
                ),
                ("gemini-3.8-flash", "Gemini 3.8 Flash: as accurate, slower"),
                ("gemini-3.5-flash-lite", "Gemini 3.5 Flash Lite"),
            ],
            Provider::OpenRouter => &[
                (
                    "google/gemini-3.1-flash-lite",
                    "Gemini 3.1 Flash Lite (recommended)",
                ),
                (
                    "google/gemini-3.8-flash",
                    "Gemini 3.8 Flash: as accurate, slower",
                ),
                (
                    "google/gemma-4-26b-a4b-it",
                    "Gemma 4 26B: cheapest, a little less accurate",
                ),
                ("anthropic/claude-haiku-5.5", "Claude Haiku 5.5"),
                ("openai/gpt-6-luna", "GPT-6 Luna"),
            ],
            Provider::OpenAi => &[("gpt-6-luna", "GPT-6 Luna (recommended)")],
            Provider::Anthropic => &[("claude-haiku-5-5", "Claude Haiku 5.5 (recommended)")],
            Provider::Custom => &[],
        }
    }

    /// Whether replies reliably follow an enforced JSON schema. Through
    /// OpenRouter and other OpenAI-compatible services that depends on the
    /// model, so there the prompt alone asks for JSON.
    pub fn enforces_schema(self) -> bool {
        matches!(self, Provider::Gemini | Provider::OpenAi)
    }

    /// Where to create an API key.
    pub fn key_page(self) -> Option<&'static str> {
        match self {
            Provider::Gemini => Some("https://aistudio.google.com/apikey"),
            Provider::OpenRouter => Some("https://openrouter.ai/keys"),
            Provider::OpenAi => Some("https://platform.openai.com/api-keys"),
            Provider::Anthropic => Some("https://console.anthropic.com/settings/keys"),
            Provider::Custom => None,
        }
    }
}

/// Which model to ask, and how hard it should think.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelChoice {
    pub provider: Provider,
    /// The provider's model name, e.g. `gemini-3.8-flash`.
    pub model: String,
    /// Reasoning effort where the model supports it (Gemini's thinking
    /// level, OpenAI's reasoning effort), e.g. "low". `None` uses the
    /// model's default.
    pub effort: Option<String>,
    /// Ask for JSON in the prompt only, without enforcing a schema. Some
    /// models answer an enforced schema badly through OpenRouter (Claude
    /// with placeholder items, Qwen by repeating the schema).
    #[serde(default)]
    pub free_form: bool,
    /// The API's address for [`Provider::Custom`], e.g.
    /// `https://api.groq.com/openai/v1`.
    #[serde(default)]
    pub base_url: Option<String>,
}

/// Marks a free-form model in a spec.
const FREE_FORM: &str = "+freeform";

impl ModelChoice {
    /// A model as the app uses it: free-form where schemas aren't reliable.
    pub fn new(provider: Provider, model: &str, base_url: Option<&str>) -> ModelChoice {
        ModelChoice {
            provider,
            model: model.trim().to_string(),
            effort: None,
            free_form: !provider.enforces_schema(),
            base_url: base_url.map(|u| u.trim().to_string()),
        }
    }

    /// Parses `provider:model`, with `@effort` and then `+freeform`
    /// optionally after it.
    pub fn parse(spec: &str) -> Option<ModelChoice> {
        let (spec, free_form) = match spec.strip_suffix(FREE_FORM) {
            Some(rest) => (rest, true),
            None => (spec, false),
        };
        let (provider, rest) = spec.split_once(':')?;
        let (model, effort) = match rest.split_once('@') {
            Some((model, effort)) => (model, Some(effort.to_string())),
            None => (rest, None),
        };
        if model.is_empty() {
            return None;
        }
        Some(ModelChoice {
            provider: Provider::from_code(provider)?,
            model: model.to_string(),
            effort,
            free_form,
            base_url: None,
        })
    }

    /// Back to `provider:model[@effort][+freeform]`.
    pub fn spec(&self) -> String {
        let mut spec = format!("{}:{}", self.provider.code(), self.model);
        if let Some(effort) = &self.effort {
            spec.push('@');
            spec.push_str(effort);
        }
        if self.free_form {
            spec.push_str(FREE_FORM);
        }
        spec
    }
}

/// What the model is asked about a photo. The JSON shape is spelled out for
/// providers that can't be given a schema. (The comparison in
/// docs/AI_PHOTO_EVAL.md used this exact text.)
pub const PROMPT: &str = r#"Identify each food and drink in this photo and estimate how much of it there is.

- Name each item generically, the way a nutrition database would, such as "chicken breast, roasted", "white rice, cooked" or "broccoli, steamed". Name a brand only if its packaging is visible.
- List the separate foods on a plate as separate items. Name a mixed dish, such as lasagna or a burrito, as one item unless its parts are clearly separate.
- Include sauces, dressings, oils, spreads and toppings you can see, as items of their own.
- Estimate the weight in grams of the edible part as served: cooked weight for cooked foods, without bones, peel or packaging. Use the plate, cutlery and the foods' own sizes for scale.
- Also estimate each item's energy (kcal), protein, fat and carbohydrate (grams) at that weight.
- When unsure, give your best guess rather than leaving an item out.

Reply with only JSON in this form:
{"items": [{"name": "white rice, cooked", "grams": 150, "kcal": 195, "protein_g": 4, "fat_g": 0.4, "carbs_g": 43}]}
If there is no food or drink in the photo, reply {"items": []}."#;

/// What the model is asked about a meal described in words, without a photo.
pub const DESCRIPTION_PROMPT: &str = r#"Identify each food and drink in the meal described below and estimate how much of it there is.

- Name each item generically, the way a nutrition database would, such as "chicken breast, roasted", "white rice, cooked" or "broccoli, steamed". Name a brand only if the description does.
- List separate foods as separate items. Name a mixed dish, such as lasagna or a burrito, as one item unless its parts are listed separately.
- Include sauces, dressings, oils, spreads and toppings that are mentioned, as items of their own.
- Estimate the weight in grams of the edible part as served: cooked weight for cooked foods, without bones, peel or packaging. Use the amounts given, or a typical portion when none is.
- Also estimate each item's energy (kcal), protein, fat and carbohydrate (grams) at that weight.
- When unsure, give your best guess rather than leaving an item out.

Reply with only JSON in this form:
{"items": [{"name": "white rice, cooked", "grams": 150, "kcal": 195, "protein_g": 4, "fat_g": 0.4, "carbs_g": 43}]}
If no food or drink is described, reply {"items": []}."#;

/// The prompt for a photo (with the user's note, if any) or, without a
/// photo, for the user's description.
pub fn prompt(photo: bool, note: Option<&str>) -> String {
    let note = note.map(str::trim).filter(|n| !n.is_empty());
    match (photo, note) {
        (true, Some(note)) => format!("{PROMPT}\n\nThe person who took the photo adds: {note}"),
        (true, None) => PROMPT.to_string(),
        (false, note) => format!("{DESCRIPTION_PROMPT}\n\nThe meal: {}", note.unwrap_or("")),
    }
}

/// One food the model saw, with its estimates for the amount shown.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub name: String,
    pub grams: f64,
    #[serde(default)]
    pub kcal: f64,
    #[serde(default)]
    pub protein_g: f64,
    #[serde(default)]
    pub fat_g: f64,
    #[serde(default)]
    pub carbs_g: f64,
}

/// The model's answer for one photo.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Estimate {
    pub items: Vec<Item>,
    /// Tokens billed, for cost estimates.
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// What the request cost in US dollars, when the provider says
    /// (OpenRouter does).
    #[serde(default)]
    pub cost_usd: Option<f64>,
}

/// An HTTP POST to send: URL, headers (including the API key) and JSON body.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

/// The JSON schema of the reply, for providers that take one.
fn reply_schema() -> Value {
    let number = json!({ "type": "number" });
    json!({
        "type": "object",
        "properties": {
            "items": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "grams": number,
                        "kcal": number,
                        "protein_g": number,
                        "fat_g": number,
                        "carbs_g": number,
                    },
                    "required": ["name", "grams", "kcal", "protein_g", "fat_g", "carbs_g"],
                    "additionalProperties": false,
                },
            },
        },
        "required": ["items"],
        "additionalProperties": false,
    })
}

/// Gemini's schema dialect: upper-case types and no `additionalProperties`.
fn gemini_schema(schema: &Value) -> Value {
    match schema {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| *k != "additionalProperties")
                .map(|(k, v)| {
                    let v = match (k.as_str(), v) {
                        ("type", Value::String(t)) => Value::String(t.to_uppercase()),
                        _ => gemini_schema(v),
                    };
                    (k.clone(), v)
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// A photo to send: its bytes and media type, e.g. `image/jpeg`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Photo<'a> {
    pub bytes: &'a [u8],
    pub mime: &'a str,
}

/// The request asking `choice`'s model about a photo, a description
/// (`note` without a photo), or both.
pub fn request(
    choice: &ModelChoice,
    api_key: &str,
    photo: Option<Photo>,
    note: Option<&str>,
) -> Request {
    let image = photo.map(|p| {
        (
            p.mime,
            base64::engine::general_purpose::STANDARD.encode(p.bytes),
        )
    });
    let text = prompt(photo.is_some(), note);
    match choice.provider {
        Provider::Gemini => {
            let mut config = if choice.free_form {
                json!({})
            } else {
                json!({
                    "responseMimeType": "application/json",
                    "responseSchema": gemini_schema(&reply_schema()),
                })
            };
            if let Some(level) = &choice.effort {
                config["thinkingConfig"] = json!({ "thinkingLevel": level });
            }
            let mut parts = Vec::new();
            if let Some((mime, data)) = &image {
                parts.push(json!({ "inline_data": { "mime_type": mime, "data": data } }));
            }
            parts.push(json!({ "text": text }));
            Request {
                url: format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
                    choice.model
                ),
                headers: vec![("x-goog-api-key".into(), api_key.into())],
                body: json!({
                    "contents": [{ "parts": parts }],
                    "generationConfig": config,
                }),
            }
        }
        Provider::Anthropic => {
            let mut content = Vec::new();
            if let Some((mime, data)) = &image {
                content.push(json!({
                    "type": "image",
                    "source": { "type": "base64", "media_type": mime, "data": data },
                }));
            }
            content.push(json!({ "type": "text", "text": text }));
            Request {
                url: "https://api.anthropic.com/v1/messages".into(),
                headers: vec![
                    ("x-api-key".into(), api_key.into()),
                    ("anthropic-version".into(), "2023-06-01".into()),
                    // Allows the call from a web page (the web preview).
                    (
                        "anthropic-dangerous-direct-browser-access".into(),
                        "true".into(),
                    ),
                ],
                body: json!({
                    "model": choice.model,
                    "max_tokens": 4096,
                    "messages": [{ "role": "user", "content": content }],
                }),
            }
        }
        Provider::OpenAi | Provider::OpenRouter | Provider::Custom => {
            let mut content = vec![json!({ "type": "text", "text": text })];
            if let Some((mime, data)) = &image {
                content.push(json!({
                    "type": "image_url",
                    "image_url": { "url": format!("data:{mime};base64,{data}") },
                }));
            }
            let mut body = json!({
                "model": choice.model,
                "messages": [{ "role": "user", "content": content }],
            });
            if !choice.free_form {
                body["response_format"] = json!({
                    "type": "json_schema",
                    "json_schema": { "name": "foods", "strict": true, "schema": reply_schema() },
                });
            }
            let mut headers = Vec::new();
            if !api_key.is_empty() {
                headers.push(("authorization".into(), format!("Bearer {api_key}")));
            }
            let url = match choice.provider {
                Provider::OpenAi => {
                    body["max_completion_tokens"] = json!(8000);
                    if let Some(effort) = &choice.effort {
                        body["reasoning_effort"] = json!(effort);
                    }
                    "https://api.openai.com/v1/chat/completions".to_string()
                }
                Provider::OpenRouter => {
                    body["max_tokens"] = json!(8000);
                    if let Some(effort) = &choice.effort {
                        body["reasoning"] = json!({ "effort": effort });
                    }
                    // Names the app on OpenRouter's activity page.
                    headers.push(("x-title".into(), "OpenMacro".into()));
                    "https://openrouter.ai/api/v1/chat/completions".to_string()
                }
                _ => {
                    body["max_tokens"] = json!(8000);
                    let base = choice.base_url.as_deref().unwrap_or_default();
                    format!("{}/chat/completions", base.trim_end_matches('/'))
                }
            };
            Request { url, headers, body }
        }
    }
}

/// The provider's error message from a failed reply, if it has one.
pub fn error_message(reply: &Value) -> Option<String> {
    let error = reply.get("error")?;
    Some(
        error
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| error.to_string()),
    )
}

/// The model's text from a successful reply, and what it cost.
struct ReplyText {
    text: String,
    input_tokens: u64,
    output_tokens: u64,
    cost_usd: Option<f64>,
}

fn reply_text(provider: Provider, reply: &Value) -> Result<ReplyText, String> {
    let n = |v: &Value| v.as_u64().unwrap_or(0);
    match provider {
        Provider::Gemini => {
            let parts = reply["candidates"][0]["content"]["parts"]
                .as_array()
                .ok_or_else(|| {
                    let reason = reply["candidates"][0]["finishReason"]
                        .as_str()
                        .or(reply["promptFeedback"]["blockReason"].as_str())
                        .unwrap_or("no answer");
                    format!("Gemini gave no answer ({reason}).")
                })?;
            let text = parts
                .iter()
                .filter(|p| p["thought"] != json!(true))
                .filter_map(|p| p["text"].as_str())
                .collect::<String>();
            let usage = &reply["usageMetadata"];
            Ok(ReplyText {
                text,
                input_tokens: n(&usage["promptTokenCount"]),
                output_tokens: n(&usage["candidatesTokenCount"]) + n(&usage["thoughtsTokenCount"]),
                cost_usd: None,
            })
        }
        Provider::Anthropic => {
            let text = reply["content"]
                .as_array()
                .ok_or("Claude gave no answer.")?
                .iter()
                .filter(|b| b["type"] == "text")
                .filter_map(|b| b["text"].as_str())
                .collect::<String>();
            let usage = &reply["usage"];
            Ok(ReplyText {
                text,
                input_tokens: n(&usage["input_tokens"]),
                output_tokens: n(&usage["output_tokens"]),
                cost_usd: None,
            })
        }
        Provider::OpenAi | Provider::OpenRouter | Provider::Custom => {
            let message = &reply["choices"][0]["message"];
            if let Some(refusal) = message["refusal"].as_str() {
                return Err(format!("The model refused: {refusal}"));
            }
            let text = message["content"]
                .as_str()
                .ok_or(format!("{} gave no answer.", provider.label()))?
                .to_string();
            let usage = &reply["usage"];
            Ok(ReplyText {
                text,
                input_tokens: n(&usage["prompt_tokens"]),
                output_tokens: n(&usage["completion_tokens"]),
                cost_usd: usage["cost"].as_f64(),
            })
        }
    }
}

/// The items in the model's JSON text, which may be wrapped in a code
/// fence or a sentence.
pub fn parse_items(text: &str) -> Result<Vec<Item>, String> {
    let start = text.find('{').ok_or("The answer had no JSON in it.")?;
    let end = text.rfind('}').ok_or("The answer had no JSON in it.")?;
    #[derive(Deserialize)]
    struct Reply {
        items: Vec<Item>,
    }
    let reply: Reply = serde_json::from_str(&text[start..=end])
        .map_err(|e| format!("The answer wasn't in the expected form: {e}"))?;
    Ok(reply
        .items
        .into_iter()
        .filter(|i| !i.name.trim().is_empty() && i.grams.is_finite() && i.grams > 0.0)
        .collect())
}

/// The model's own text in a successful reply, to see what it said.
pub fn answer_text(provider: Provider, reply: &Value) -> Option<String> {
    reply_text(provider, reply).ok().map(|r| r.text)
}

/// Reads a successful reply from `provider`.
pub fn parse(provider: Provider, reply: &Value) -> Result<Estimate, String> {
    if let Some(message) = error_message(reply) {
        return Err(message);
    }
    let reply = reply_text(provider, reply)?;
    Ok(Estimate {
        items: parse_items(&reply.text)?,
        input_tokens: reply.input_tokens,
        output_tokens: reply.output_tokens,
        cost_usd: reply.cost_usd,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_choices_round_trip() {
        let choice = ModelChoice::parse("gemini:gemini-3.8-flash@low").unwrap();
        assert_eq!(choice.provider, Provider::Gemini);
        assert_eq!(choice.model, "gemini-3.8-flash");
        assert_eq!(choice.effort.as_deref(), Some("low"));
        assert_eq!(choice.spec(), "gemini:gemini-3.8-flash@low");
        assert_eq!(
            ModelChoice::parse("anthropic:claude-haiku-5-5")
                .unwrap()
                .effort,
            None
        );
        let free = ModelChoice::parse("openrouter:qwen/qwen3.8-flash@low+freeform").unwrap();
        assert_eq!(free.model, "qwen/qwen3.8-flash");
        assert_eq!(free.effort.as_deref(), Some("low"));
        assert!(free.free_form);
        assert_eq!(free.spec(), "openrouter:qwen/qwen3.8-flash@low+freeform");
        let body = request(&free, "k", None, Some("a banana")).body;
        assert!(body.get("response_format").is_none());
        assert!(ModelChoice::parse("nobody:model").is_none());
        assert!(ModelChoice::parse("openai:").is_none());
    }

    #[test]
    fn requests_carry_the_image_key_and_schema() {
        let image = [1u8, 2, 3];
        let gemini = request(
            &ModelChoice::parse("gemini:gemini-3.8-flash@low").unwrap(),
            "k",
            Some(Photo {
                bytes: &image,
                mime: "image/png",
            }),
            Some("about 200 g of rice"),
        );
        assert!(gemini.url.ends_with("/gemini-3.8-flash:generateContent"));
        assert_eq!(gemini.headers, vec![("x-goog-api-key".into(), "k".into())]);
        let parts = &gemini.body["contents"][0]["parts"];
        assert_eq!(parts[0]["inline_data"]["data"], "AQID");
        assert!(
            parts[1]["text"]
                .as_str()
                .unwrap()
                .ends_with("about 200 g of rice")
        );
        let config = &gemini.body["generationConfig"];
        assert_eq!(config["thinkingConfig"]["thinkingLevel"], "low");
        assert_eq!(config["responseSchema"]["type"], "OBJECT");
        assert!(
            config["responseSchema"]
                .get("additionalProperties")
                .is_none()
        );

        let openai = request(
            &ModelChoice::parse("openai:gpt-mini").unwrap(),
            "k",
            Some(Photo {
                bytes: &image,
                mime: "image/png",
            }),
            None,
        );
        assert_eq!(openai.headers[0].1, "Bearer k");
        assert_eq!(
            openai.body["messages"][0]["content"][1]["image_url"]["url"],
            "data:image/png;base64,AQID"
        );
        assert!(openai.body.get("reasoning_effort").is_none());

        let claude = request(
            &ModelChoice::parse("anthropic:claude-haiku-5-5").unwrap(),
            "k",
            Some(Photo {
                bytes: &image,
                mime: "image/png",
            }),
            None,
        );
        assert_eq!(
            claude.body["messages"][0]["content"][0]["source"]["data"],
            "AQID"
        );

        let routed = request(
            &ModelChoice::parse("openrouter:google/gemini-3.8-flash@low").unwrap(),
            "k",
            Some(Photo {
                bytes: &image,
                mime: "image/png",
            }),
            None,
        );
        assert_eq!(routed.url, "https://openrouter.ai/api/v1/chat/completions");
        assert_eq!(routed.body["model"], "google/gemini-3.8-flash");
        assert_eq!(routed.body["reasoning"]["effort"], "low");
        assert!(routed.body.get("max_completion_tokens").is_none());
    }

    #[test]
    fn descriptions_and_custom_services() {
        let choice = ModelChoice::new(
            Provider::Custom,
            " llama-vision ",
            Some("http://127.0.0.1:11434/v1/"),
        );
        assert!(choice.free_form);
        assert_eq!(choice.model, "llama-vision");
        let described = request(&choice, "", None, Some("two eggs on toast"));
        assert_eq!(described.url, "http://127.0.0.1:11434/v1/chat/completions");
        assert!(described.headers.is_empty());
        let content = described.body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        let text = content[0]["text"].as_str().unwrap();
        assert!(text.starts_with("Identify each food and drink in the meal described below"));
        assert!(text.ends_with("The meal: two eggs on toast"));

        assert!(!ModelChoice::new(Provider::Gemini, "gemini-3.1-flash-lite", None).free_form);
        let gemini = request(
            &ModelChoice::new(Provider::Gemini, "gemini-3.1-flash-lite", None),
            "k",
            None,
            Some("an apple"),
        );
        let parts = gemini.body["contents"][0]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 1);
    }

    #[test]
    fn replies_parse_for_each_provider() {
        let items = r#"{"items": [{"name": "rice", "grams": 150, "kcal": 195, "protein_g": 4, "fat_g": 0.4, "carbs_g": 43}]}"#;
        let gemini = json!({
            "candidates": [{ "content": { "parts": [
                { "text": "thinking...", "thought": true },
                { "text": items },
            ] } }],
            "usageMetadata": { "promptTokenCount": 300, "candidatesTokenCount": 40, "thoughtsTokenCount": 60 },
        });
        let estimate = parse(Provider::Gemini, &gemini).unwrap();
        assert_eq!(estimate.items[0].name, "rice");
        assert_eq!((estimate.input_tokens, estimate.output_tokens), (300, 100));

        let claude = json!({
            "content": [{ "type": "text", "text": format!("```json\n{items}\n```") }],
            "usage": { "input_tokens": 500, "output_tokens": 50 },
        });
        assert_eq!(
            parse(Provider::Anthropic, &claude).unwrap().items[0].grams,
            150.0
        );

        let openai = json!({
            "choices": [{ "message": { "content": items } }],
            "usage": { "prompt_tokens": 400, "completion_tokens": 30 },
        });
        assert_eq!(parse(Provider::OpenAi, &openai).unwrap().items.len(), 1);

        let open_router = json!({
            "choices": [{ "message": { "content": items } }],
            "usage": { "prompt_tokens": 400, "completion_tokens": 30, "cost": 0.00042 },
        });
        assert_eq!(
            parse(Provider::OpenRouter, &open_router).unwrap().cost_usd,
            Some(0.00042)
        );

        let error = json!({ "error": { "message": "API key not valid" } });
        assert_eq!(
            parse(Provider::Gemini, &error).unwrap_err(),
            "API key not valid"
        );
    }

    #[test]
    fn empty_and_weightless_items_are_dropped() {
        let items = parse_items(
            r#"{"items": [{"name": "", "grams": 10}, {"name": "tea", "grams": 0}, {"name": "toast", "grams": 30}]}"#,
        )
        .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kcal, 0.0);
        assert!(parse_items("no food here").is_err());
    }
}
