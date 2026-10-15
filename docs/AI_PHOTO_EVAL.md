# AI photo logging: model comparison

Which vision model should OpenMacro use to log food from a photo? We
measured small, cheap models on real photos rather than relying on
published benchmarks, which lag a model generation behind.

## Method

- **Photos:** 100 overhead photos from the test split of Google's
  [Nutrition5k](https://github.com/google-research-datasets/Nutrition5k)
  dataset (CC BY 4.0). Every dish was weighed and its nutrients computed
  from its weighed ingredients. Dishes range from 31 to 928 kcal (median
  304 kcal, 193 g).
- **Prompt:** the one in `crates/openmacro-ai` that the app will use. The
  model lists each food with its weight in grams and its own estimate of
  calories and macros.
- **Two ways to get calories:**
  - **Model:** the model's own estimate.
  - **Database:** each food matched by name to OpenMacro's food database,
    with nutrients taken from the match.
- **Calls:** all models through OpenRouter, October 2026.
  - *Free-form* asks for JSON in the prompt without enforcing a schema.
    Through OpenRouter, Claude Haiku answered an enforced schema with
    placeholder items for over half the photos, and Qwen sometimes repeated
    the schema back.
  - Each model is listed below with whichever mode worked best for it.
- **Measures:**
  - Average calories off per dish, and the median percentage error.
  - CCC, the concordance correlation coefficient: 1 means perfect
    agreement, and it is lowered by both scatter and bias.
  - Weight error, time per photo, and the cost OpenRouter billed.

The whole comparison cost $0.75.

## Results

| Model | Calories off (model) | Median error | CCC | Calories off (database) | Weight: median error | Time | Per 1,000 photos |
|---|---|---|---|---|---|---|---|
| Gemini 3.8 Flash, low thinking | 88 kcal | 27% | 0.80 | 116 kcal | 21% | 5.1 s | $2.08 |
| **Gemini 3.1 Flash Lite** | 88 kcal | 28% | 0.80 | 104 kcal | 23% | 2.4 s | $0.67 |
| Gemini 3.1 Flash Lite, free-form | 93 kcal | 26% | 0.78 | 97 kcal | 18% | 2.4 s | $0.57 |
| Gemini 3.5 Flash Lite | 98 kcal | 30% | 0.75 | 104 kcal | 22% | 2.3 s | $0.93 |
| Gemma 4 26B (open weights), free-form | 98 kcal | 29% | 0.75 | 117 kcal | 22% | 4.7 s | $0.08 |
| Qwen 3.8 Flash, low thinking, free-form | 103 kcal | 36% | 0.72 | 111 kcal | 30% | 19.6 s | $0.60 |
| Claude Haiku 5.5, free-form | 109 kcal | 29% | 0.75 | 126 kcal | 37% | 5.0 s | $0.45 |
| GPT-6 Luna, low thinking | 109 kcal | 33% | 0.74 | 121 kcal | 28% | 6.7 s | $0.25 |

Gemma answered 99 of 100 photos and Qwen 99; every other model answered
all 100.

**Baseline:** we also matched each dish's *true* ingredient names and
weights to the database. That gave 65 kcal off (median 13%, CCC 0.88).
This is the error the matching alone adds, mostly from terse names: for
example, "eggs" matched a frozen egg mixture.

## Findings

- **The Gemini models are the most accurate and the fastest.** With 100
  photos, the differences among the top four are within noise: the
  average error has a margin of roughly ±9 kcal. Our numbers match the
  2026 study by Sterling et al., where Gemini 3.0 Flash had CCC 0.77 and
  was 81 kcal off.
- **Cost is negligible for every model tested.** At three photos a day,
  Gemini 3.1 Flash Lite costs about 6 cents a month and Gemini 3.8 Flash
  about 19 cents.
- **Portions are the main source of error.** The Gemini models
  underestimate weight by about 10% on average.
- **The model's own calorie estimate beat database matching for every
  model.** Matching a food *name* to a database food adds error of its
  own: "roasted potatoes" matched a frozen, unprepared product, and
  olives matched a branded jar. Database nutrients only help when the
  match is right.
- **A capable open-weights model exists.** Gemma 4 26B was close to the
  Gemini models at a tenth of the price, though it is far too large to run
  on a phone.

## Recommendation

- **Default to Gemini 3.1 Flash Lite.** It ties for the most accurate, is
  the fastest, and costs well under a cent a photo.
- **Keep the model a setting,** because new models arrive every few months.
- **Use the model's own nutrients** for each item unless a database match
  is clearly the same food.
- **Let the user adjust weights** before logging.

## Rerunning

```bash
cargo run --release -p openmacro-ai --features eval --bin food-photo-eval -- fetch data/nutrition5k
cargo run --release -p openmacro-ai --features eval --bin food-photo-eval -- models --keys ~/.openmacro-eval/keys.env --filter flash
cargo run --release -p openmacro-ai --features eval --bin food-photo-eval -- run data/nutrition5k --keys ~/.openmacro-eval/keys.env --model openrouter:google/gemini-3.1-flash-lite
cargo run --release -p openmacro-ai --features eval --bin food-photo-eval -- report data/nutrition5k --foods crates/openmacro-app/assets/foods.db
```

Answers are saved in `data/nutrition5k/runs`, so a rerun only asks about
new photos, and the report can be redone (for example after improving the
matching) at no cost.
