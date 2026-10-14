# OpenMacro feature specification

This document describes what OpenMacro should do, written from observing how existing adaptive-coaching nutrition apps behave. It is a behavioral spec in our own words. It contains no third-party code, copy, or assets.

Priority tags:
- **P0**: MVP. The app is not useful without it.
- **P1**: needed for parity with the apps that inspired this one.
- **P2**: nice to have.

---

## 1. App structure

There are five bottom tabs:

| Tab | Purpose |
|---|---|
| Dashboard | A summary banner plus customizable insight tiles |
| Food Log | A timeline of the selected day's food |
| **+** (center) | The shortcuts sheet: search, barcode, AI, weight, recipes, quick add, photos, metrics |
| Strategy | Check-in countdown, active program, goal, goal history |
| More | Settings, data management, support, about |

A floating "Search for a food" bar with barcode and AI buttons sits above the tab bar on the main tabs.

---

## 2. Food logging

### 2.1 Timeline log (P0)
- The day is shown as a vertical list of clock hours. The hour range is configurable, with a default of 7 AM to 11 PM.
- Each hour has a "+" button that adds food at that hour. Entries show their timestamp.
- A week strip at the top selects the day. Each day pill shows a ring for calorie progress.
- A header shows calories, protein, fat and carbs as consumed against target, with a toggle between "consumed" and "remaining".
- There are no fixed meal slots: time-of-day *is* the grouping.
- Optional extras: per-hour macro subtotals, and a setting for whether overages show as negative "remaining" numbers.

### 2.2 The Plate: staging before logging (P0)
Every way of adding food puts items on a **plate** first. The plate is a draft list that the user reviews, then logs to a chosen date and time in one action.

- The plate header shows the target time, the running total against today's calorie target, and icons for the items.
- A nutrition panel can switch between "Plate" (this plate only) and "Day" (the day's totals plus the plate).
- Each item has an inline quantity field and a unit picker.
- Leaving with a non-empty plate asks for confirmation before discarding it.

The logger sheet has five tabs:

| Tab | Behavior | Priority |
|---|---|---|
| **Search** | Text search across the food database. Results are grouped as *Common* (generic foods) and *Branded*, with "see more" on each group. Each row shows calories, P/F/C and the default serving, and has a one-tap add. History, favorites, custom foods and recipes appear as their own sections. | P0 |
| **Scan** | Two modes: *Barcode* (live camera with a torch toggle) and *Label* (photograph a nutrition label, AI-assisted). A keyboard button opens manual barcode entry with a "Look up" button. When a barcode isn't found, the user can create a custom food with that barcode. | P0 (barcode), P2 (label) |
| **Quick Add** | Enter energy (kcal or kJ) plus optional protein, fat, carbs and alcohol in grams. Shows the calories implied by the macro grams for comparison. | P0 |
| **Library** | The user's Recipes, custom Foods and Favorites, with a filter box and a "+" to create new items. | P0 |
| **AI** | *Snap*: photograph a meal. *Describe*: free text of up to about 500 characters, plus up to 3 photos. The AI returns plate items mapped onto real database foods with quantities and units. Composite dishes expand into editable ingredient rows. A disclaimer asks users to verify the results. Users bring their own API key, and nothing is sent unless they turn it on. | P2 (pluggable provider) |

### 2.3 Food detail (P0)
- **Header:** calories, then protein, fat and carbs in grams, each with its percentage of the food's calories.
- **Actions:** favorite, and "copy to custom food" (to make an editable copy).
- **Impact on targets:** a ring for each macro showing what percentage of today's target this serving uses.
- **Full nutrient panel:** see §2.6 for the sections.
- **Amount entry:** a numeric keypad that accepts fractions ("1 1/2", "3/4") and offers unit chips (g, oz, and the food's named servings such as "1 container" or "1 slice").

### 2.4 Editing the log (P1)
- **Move or copy:** move an entry to another hour; copy or paste entries to another hour, date, or several dates.
- **Delete:** multi-select delete, swipe-to-delete with undo, and clear the whole day.
- **Recipes:** turn selected entries into a recipe, or split a logged recipe back into its ingredients.
- **Day notes:** free-text notes attached to a day.

### 2.5 Custom foods and recipes (P0 custom foods, P1 recipes)
- **Custom food:** a 4-step form:
  1. how to build it (from scratch or AI-assisted);
  2. name (required), brand and an optional barcode with a Scan button;
  3. nutrition basis (per serving, per 100 g or per 100 ml), serving weight in grams and serving size (e.g. "4 pieces");
  4. nutrients, in a US-label, non-US-label or full-detail layout. Energy can be kcal or kJ, and sodium can be entered as salt.

  Finish with "Create" or "Create & Add" (also adds it to the plate).
- **Recipe creation:** one of three methods:
  - build from scratch: name, number of servings, optional total cooked weight, and ingredients;
  - import from a web link (P1, by parsing schema.org `Recipe` markup);
  - import with AI (P2).
- **Recipe contents:** recipes can contain other recipes.
- **Sharing:** recipes can be shared by link (P2).

### 2.6 Nutrients tracked (P0 data model, P1 UI)
There are 55 nutrients in total:

| Group | Nutrients |
|---|---|
| Energy | Calories |
| Macros | Protein, Fat, Carbs, Alcohol |
| Carb breakdown | Fiber, Starch, Sugars, Added sugars. Net carbs = carbs − fiber (derived). |
| Fat breakdown | Monounsaturated, Polyunsaturated, Omega-3 (ALA, DHA, EPA), Omega-6, Saturated, Trans |
| Protein breakdown (essential amino acids) | Cysteine, Histidine, Isoleucine, Leucine, Lysine, Methionine, Phenylalanine, Threonine, Tryptophan, Tyrosine, Valine |
| Vitamins | B1, B2, B3, B5, B6, B12, Folate, A, C, D, E, K |
| Minerals | Calcium, Copper, Iron, Magnesium, Manganese, Phosphorus, Potassium, Selenium, Sodium, Zinc |
| Other | Caffeine, Cholesterol, Choline, Water |

Each micronutrient can have a **floor**, a **target** and a **ceiling**:
- floor: lower threshold intake;
- target: roughly the RDA or AI;
- ceiling: the tolerable upper limit.

By default these are set automatically from age and sex, and each one can be overridden. Goal changes are stored as dated history.

### 2.7 Food database (P0)
Our implementation plan:
- **Generic foods:** USDA FoodData Central (public domain) for Foundation, SR Legacy and FNDDS foods.
- **Branded foods:** Open Food Facts (ODbL; attribution required, and the database stays open), plus USDA Branded Foods.
- **Search:** local full-text search. The ranking boosts the user's own recent and frequent foods, and the user's custom foods and recipes.

---

## 3. Body and weight tracking

| Feature | Notes | Priority |
|---|---|---|
| Scale weight | Multiple weigh-ins per day allowed (averaged). Body fat % optional. | P0 |
| Trend weight | A smoothed weight line; see [ALGORITHMS.md](ALGORITHMS.md). Users can pick the smoothing speed. | P0 |
| Body measurements | Neck, shoulders, chest, waist, hips, and left/right biceps, forearms, wrists, thighs, calves and ankles. Waist-to-height and waist-to-hip ratios are derived. | P1 |
| Progress photos | Front, side and back poses, with before/after side-by-side and overlay comparison | P1 |
| Steps | Read from the health platform | P1 |
| Water | Logged as a nutrient | P1 |
| Period | A yes/no flag per day | P2 |

---

## 4. Coaching

### 4.1 Expenditure (P0)
- The app keeps a daily estimate of total energy expenditure, calculated from **logged intake and weight change only**. Adherence to targets plays no part.
- **Starting point:** a BMR equation × an activity factor. The inputs are sex, age, height, weight, optional body fat, activity level and exercise frequency.
- **When it holds:** while data is insufficient, the estimate **holds** at its last value. Insufficient means too few logged days in the recent window, or no weigh-in in the past 7 days. Charts mark these days as "holding".
- **Fasted days:** can be marked, and count as 0 kcal.
- **Partially logged days:** can be marked, and are excluded.
- **Unmarked empty days:** are ignored.
- **The chart:**
  - time ranges 1W, 1M, 3M, 6M, 1Y and All;
  - daily, weekly or monthly grouping;
  - the average and the change over the range;
  - an uncertainty band and markers for holding days;
  - "change over 3/7/14/28 days" summaries.
- **Settings:**
  - Dynamic or fixed (static) expenditure;
  - calculation start date;
  - BMR equation choice;
  - optional step-informed updates (P2).

### 4.2 Goals (P0)
- **Types:** Lose, Maintain and Gain.
- **Rate:** expressed as **% of body weight per week**. The UI also shows the equivalent kg or lb per week.
- **Goal weight:** shows a projected completion date based on the trend.
- **Maintain:** keeps the trend weight inside a band and gently corrects when it drifts outside.
- **History:** goals are kept as a dated list, and a previous goal can be reopened.

### 4.3 Programs (P0)
Programs come in three styles:

| Style | User sets | App sets |
|---|---|---|
| Coached | Diet type, protein preference, training type, calorie floor, weekly distribution | All calories and macros, updated at every check-in |
| Collaborative | Protein and the carb/fat preference | Calories, updated at every check-in |
| Manual | All calories and macros (per weekday optional) | Nothing (progress is still tracked) |

The Coached wizard has six steps:
1. **Diet type:** Balanced, Low-fat, Low-carb or Keto.
2. **Calorie floor:** Standard (never below 1200 kcal) or Low (never below 800 kcal, with a warning).
3. **Training:** None/relaxed, Lifting, Cardio, or Cardio & Lifting.
4. **Weekly distribution:** distribute evenly, or shift calories so chosen weekdays get more while the weekly average stays the same.
5. **Higher-calorie days:** if shifting, pick which weekdays get more.
6. **Protein:** Low, Moderate, High or Extra High.

After the wizard, an explainer walks through how the targets were derived: expenditure, then average target, then the shift, then protein in g/kg, then the carb/fat split.

The program card shows a 7-day bar chart with calories and P/F/C for each weekday.

### 4.4 Weekly check-in (P0)
- The check-in falls on a weekday the user picks. A countdown ring is shown, and an early check-in is allowed.
- A check-in runs a sequence of modules that appear when relevant:
  1. **Weigh-in:** prompts for a weigh-in if one is missing.
  2. **Partial logging:** shows suspiciously low days and lets the user mark them partial.
  3. **Fasting:** shows zero-calorie days and lets the user confirm them as fasted.
  4. **Logging break:** explains that expenditure is holding after a gap.
  5. **Program update:** shows the new targets and what changed.
- Optional modules can be switched off individually, and a "fast check-in" mode skips them all.

---

## 5. Dashboard (P1)

- **Summary banner** (carousel; the user chooses which comes first):
  - Weekly Nutrition: a 7-day grid of calories and P/F/C, consumed or remaining;
  - Energy Balance: nutrition − expenditure over the last 30 days;
  - Daily Nutrition.
- **Insight tiles:** Expenditure, Weight Trend, Energy Balance, Goal Progress, Macros, Nutrient Explorer (nutrient intake against floor, target and ceiling, plus top contributing foods), Scale Weight, Steps, and Body Metrics.
- **Habit tiles:** weigh-in and food-logging streaks, shown as "x/7 this week" plus a heatmap.
- **Customization:** tiles can be added, removed and reordered, and there is a "see all" gallery.

---

## 6. Data and integrations

| Feature | Priority |
|---|---|
| Local-first storage. Works fully offline. | P0 |
| Optional self-hostable sync (see [ARCHITECTURE.md](ARCHITECTURE.md)) | P2 |
| Health Connect / Apple Health: read weight, body fat, steps and nutrition; write weight, body fat, nutrition and hydration | P1 |
| Export everything (CSV and XLSX): expenditure, scale and trend weight, nutrition per day, food log, custom foods, recipes, notes | P0 (CSV), P1 (XLSX) |
| Import from other apps' exports | P2 |
| Granular delete per data type, plus full account and data deletion | P1 |

---

## 7. App settings

| Setting | Options | Priority |
|---|---|---|
| Units | Weight (kg/lb), energy (kcal/kJ), height (cm/ft-in), clock (12/24 h), distance | P0 |
| Theme | Light, dark or system | P0 |
| Accent colour and alternate app icons | | P2 |
| Language | i18n-ready strings from day one | P1 |
| Logger options | Hour range, alignment, "+" per hour, timestamps, hourly totals, week banner | P1 |
| Shortcuts | Customizable shortcuts sheet | P2 |
| Home-screen widgets and launcher shortcuts | Calorie/macro ring and bars | P2 |

---

## 8. Distribution and updates (P0)

- **Release builds:** signed release APKs are built by CI and published as GitHub Releases. All updates must be signed with the same key.
- **In-app updater:** on launch the app checks the latest GitHub Release. If it's newer, the app shows "Update available" with the release notes, downloads the APK and hands it to Android's package installer. Android asks the user once to allow installs from OpenMacro, and the user confirms each update.
- **Later:** Google Play or F-Droid listings for silent updates.

## 9. Out of scope (for now)

- A workout / strength-training logger. The reference app ships this as a separate product.
- Paywalls, subscriptions and usage quotas. OpenMacro is free.
- Mini-games and other easter eggs.
