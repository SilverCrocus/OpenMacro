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
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Gemini, Provider::Anthropic, Provider::OpenAi];

    pub fn code(self) -> &'static str {
        match self {
            Provider::Gemini => "gemini",
            Provider::Anthropic => "anthropic",
            Provider::OpenAi => "openai",
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
        }
    }

    /// The usual environment variable for the provider's API key.
    pub fn key_variable(self) -> &'static str {
        match self {
            Provider::Gemini => "GEMINI_API_KEY",
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            Provider::OpenAi => "OPENAI_API_KEY",
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
}

impl ModelChoice {
    /// Parses `provider:model` or `provider:model@effort`.
    pub fn parse(spec: &str) -> Option<ModelChoice> {
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
        })
    }

    /// Back to `provider:model[@effort]`.
    pub fn spec(&self) -> String {
        match &self.effort {
            Some(effort) => format!("{}:{}@{effort}", self.provider.code(), self.model),
            None => format!("{}:{}", self.provider.code(), self.model),
        }
    }
}

/// What the model is asked. The JSON shape is spelled out for providers
/// that can't be given a schema.
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

/// The prompt, with the user's note about the meal if they wrote one.
pub fn prompt(note: Option<&str>) -> String {
    match note.map(str::trim).filter(|n| !n.is_empty()) {
        Some(note) => format!("{PROMPT}\n\nThe person who took the photo adds: {note}"),
        None => PROMPT.to_string(),
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

/// The request asking `choice`'s model about one photo.
pub fn request(
    choice: &ModelChoice,
    api_key: &str,
    image: &[u8],
    mime: &str,
    note: Option<&str>,
) -> Request {
    let data = base64::engine::general_purpose::STANDARD.encode(image);
    let text = prompt(note);
    match choice.provider {
        Provider::Gemini => {
            let mut config = json!({
                "responseMimeType": "application/json",
                "responseSchema": gemini_schema(&reply_schema()),
            });
            if let Some(level) = &choice.effort {
                config["thinkingConfig"] = json!({ "thinkingLevel": level });
            }
            Request {
                url: format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
                    choice.model
                ),
                headers: vec![("x-goog-api-key".into(), api_key.into())],
                body: json!({
                    "contents": [{
                        "parts": [
                            { "inline_data": { "mime_type": mime, "data": data } },
                            { "text": text },
                        ],
                    }],
                    "generationConfig": config,
                }),
            }
        }
        Provider::Anthropic => Request {
            url: "https://api.anthropic.com/v1/messages".into(),
            headers: vec![
                ("x-api-key".into(), api_key.into()),
                ("anthropic-version".into(), "2023-06-01".into()),
            ],
            body: json!({
                "model": choice.model,
                "max_tokens": 4096,
                "messages": [{
                    "role": "user",
                    "content": [
                        {
                            "type": "image",
                            "source": { "type": "base64", "media_type": mime, "data": data },
                        },
                        { "type": "text", "text": text },
                    ],
                }],
            }),
        },
        Provider::OpenAi => {
            let mut body = json!({
                "model": choice.model,
                "max_completion_tokens": 8000,
                "messages": [{
                    "role": "user",
                    "content": [
                        { "type": "text", "text": text },
                        {
                            "type": "image_url",
                            "image_url": { "url": format!("data:{mime};base64,{data}") },
                        },
                    ],
                }],
                "response_format": {
                    "type": "json_schema",
                    "json_schema": { "name": "foods", "strict": true, "schema": reply_schema() },
                },
            });
            if let Some(effort) = &choice.effort {
                body["reasoning_effort"] = json!(effort);
            }
            Request {
                url: "https://api.openai.com/v1/chat/completions".into(),
                headers: vec![("authorization".into(), format!("Bearer {api_key}"))],
                body,
            }
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

/// The model's text and the tokens billed, from a successful reply.
fn reply_text(provider: Provider, reply: &Value) -> Result<(String, u64, u64), String> {
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
            Ok((
                text,
                n(&usage["promptTokenCount"]),
                n(&usage["candidatesTokenCount"]) + n(&usage["thoughtsTokenCount"]),
            ))
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
            Ok((text, n(&usage["input_tokens"]), n(&usage["output_tokens"])))
        }
        Provider::OpenAi => {
            let message = &reply["choices"][0]["message"];
            if let Some(refusal) = message["refusal"].as_str() {
                return Err(format!("The model refused: {refusal}"));
            }
            let text = message["content"]
                .as_str()
                .ok_or("OpenAI gave no answer.")?
                .to_string();
            let usage = &reply["usage"];
            Ok((
                text,
                n(&usage["prompt_tokens"]),
                n(&usage["completion_tokens"]),
            ))
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

/// Reads a successful reply from `provider`.
pub fn parse(provider: Provider, reply: &Value) -> Result<Estimate, String> {
    if let Some(message) = error_message(reply) {
        return Err(message);
    }
    let (text, input_tokens, output_tokens) = reply_text(provider, reply)?;
    Ok(Estimate {
        items: parse_items(&text)?,
        input_tokens,
        output_tokens,
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
        assert!(ModelChoice::parse("nobody:model").is_none());
        assert!(ModelChoice::parse("openai:").is_none());
    }

    #[test]
    fn requests_carry_the_image_key_and_schema() {
        let image = [1u8, 2, 3];
        let gemini = request(
            &ModelChoice::parse("gemini:gemini-3.8-flash@low").unwrap(),
            "k",
            &image,
            "image/png",
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
            &image,
            "image/png",
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
            &image,
            "image/png",
            None,
        );
        assert_eq!(
            claude.body["messages"][0]["content"][0]["source"]["data"],
            "AQID"
        );
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
