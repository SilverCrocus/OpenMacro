# OpenMacro algorithms

These are OpenMacro's own designs, built from public, standard methods. They are implemented in [`crates/openmacro-core`](../crates/openmacro-core/src). Every constant here is a default that we expect to tune.

Throughout:
- **ρ** is the energy content of body-mass change: **7700 kcal/kg** (the classic 3500 kcal/lb).
- All internal values are metric.

---

## 1. Trend weight — `trend.rs`

Scale weight is noisy: water, glycogen and gut contents easily move it by a kilogram or more from day to day. Trend weight is the signal underneath that noise.

The method is an exponential moving average (EMA) over daily values:

```text
trend[0] = first weigh-in
trend[d] = trend[d-1] + α · (x[d] - trend[d-1])
```

`x[d]` is chosen as follows:
- If the user weighed in on day `d`, `x[d]` is that day's average weigh-in.
- If they skipped day `d`, `x[d]` is a linear interpolation between the weigh-ins before and after it. Gaps therefore neither freeze the trend nor make it jump.
- After the last weigh-in, the trend holds flat.

| Preset | α |
|---|---|
| Faster | 0.20 |
| Standard (default) | 0.10 |
| Slower | 0.05 |

**Planned (v2):** a local-linear-trend Kalman filter (state: level and slope) with a Rauch–Tung–Striebel backward smoother. The smoother revises past trend values when new weigh-ins arrive, which reduces lag. The filter's variance also gives a ±2σ "noise band" for charts.

---

## 2. Initial expenditure — `bmr.rs`

Before any data exists, the starting estimate is BMR × an activity factor.

| Equation | Formula | Needs body fat? |
|---|---|---|
| Mifflin-St Jeor (default) | 10·kg + 6.25·cm − 5·age + 5 (men) / −161 (women) | no |
| Katch-McArdle | 370 + 21.6·LBM | yes |
| Cunningham | 500 + 22·LBM | yes |

Here LBM = weight × (1 − body-fat fraction). If the user picks a lean-mass equation but hasn't given body fat, Mifflin-St Jeor is used instead.

| Activity | Factor |
|---|---|
| Sedentary | 1.2 |
| Light | 1.375 |
| Moderate | 1.55 |
| Very | 1.725 |
| Extreme | 1.9 |

This number only matters for the first few days. After that, §3 takes over.

---

## 3. Adaptive expenditure — `expenditure.rs`

Energy balance gives `intake − expenditure = ρ · Δmass/Δt`. Rearranged, each day produces a **measurement** from a trailing window:

```text
raw[d] = mean(usable intake in window) − ρ · slope(scale weight in window)
```

The slope is a least-squares fit to the **raw weigh-ins** in the window, in kg/day. It is deliberately *not* taken from the smoothed trend line. An EMA trend starts flat at the first weigh-in, so for the first few weeks it understates how fast weight is changing. In testing, that biased new users' expenditure low by 100–300 kcal and gave them targets that were too low.

Raw weigh-ins are noisy, so each measurement is blended in with a one-dimensional Kalman update:

```text
P      += q²                                       expenditure can drift a little each day
R       = ((ρ·σ_w)² / Σ(x−x̄)² + σ_i² / n) · W · k   how uncertain this window's measurement is
K       = P / (P + R)                              how much to trust it
E      += clamp(K · (raw − E), ±cap)
P      *= 1 − K
```

The terms:
- `σ_w` is the day-to-day scale noise, and `Σ(x−x̄)²` is the spread of the weigh-in days.
- `σ_i` is the day-to-day intake variation, and `n` is the number of usable intake days.
- `W · k` corrects for consecutive windows sharing most of their days: W is the window length and k the overlap factor.

The effect is intuitive. Three weigh-ins over three days barely move the estimate, while two weeks of daily weigh-ins move it a lot. `√P` is reported with every point as a 1σ uncertainty band for charts.

**The estimate depends only on what was eaten and how weight changed.** Whether the user hit their targets doesn't matter, so the estimate stays correct when they're off plan.

### Which days count

| Day | Intake used |
|---|---|
| Logged normally | the logged total |
| Nothing logged | ignored (excluded, *not* treated as zero) |
| Marked **fasted** | 0 kcal |
| Marked **partial** | ignored |

### Hold rule

The estimate **holds** at its previous value (while its uncertainty keeps growing) on any day where any of these is true:
- fewer than `min_intake_days` days in the window have usable intake;
- the window has fewer than two weigh-ins on different days;
- the user hasn't weighed in within the last `weigh_in_lookback_days`.

Until the first successful update, the status is `Initial`. After that, a day that fails the check is `Holding`.

### Defaults

These were chosen by simulation: 200 simulated people with Gaussian scale noise (σ 0.6 kg) and intake noise (σ 250 kcal), priors 350 kcal too high or too low, and both a constant expenditure and a 200 kcal drop at day 45.

| Parameter | Default |
|---|---|
| `window_days` (W) | 14 |
| `min_intake_days` | 3 (the first update can happen on day 3) |
| `weigh_in_lookback_days` | 7 |
| `weight_noise_kg` (σ_w) | 0.6 |
| `intake_noise_kcal` (σ_i) | 350 |
| `process_noise_kcal` (q) | 40 per day |
| `initial_uncertainty_kcal` | 350 |
| `overlap_factor` (k) | 0.5 |
| `max_daily_change_kcal` | 150 |

Over days 14–89 of those simulations, the RMS error was **115 kcal** for this estimator against **154 kcal** for the earlier trend-slope version.

The reported band is conservative: by day 89 it is about ±220 kcal, while the actual errors are about ±40. It should be calibrated against real data before it is shown as a confidence interval.

### Tested behaviour

- **Convergence:** a simulated person has a true expenditure of 2500 kcal, eats 2000 ± 200 kcal, and has ±0.5 kg of water noise on the scale. Starting from a wrong prior of 2200, the estimate converges to within 60 kcal of the truth in 90 days.
- **No early drag:** starting from the correct prior, the estimate stays within ±120 kcal of the truth throughout the first four weeks.
- **Uncertainty:** the band shrinks as data accumulates.
- **Logging stops:** the estimate updates while ≥ 3 logged days remain in the window, then holds.
- **No weigh-ins:** the estimate never leaves its initial value.

### Planned

- Calibrating the uncertainty band.
- Step-informed updates: use changes in step count as an early signal that expenditure is moving.
- A predictive adjustment when the goal changes (metabolic adaptation).

---

## 4. Calorie targets — `targets.rs`

### Average daily target

```text
weekly change (kg) = trend weight × rate% / 100      (rate < 0 loses, > 0 gains)
daily delta (kcal) = weekly change × ρ / 7
target             = max(expenditure + daily delta, floor)
```

| Floor | kcal |
|---|---|
| Standard (default) | 1200 |
| Low | 800 |
| None | — |

When the floor is binding, the result is flagged `limited_by_floor`, and the UI should suggest a slower rate.

Example: expenditure 2128 kcal, weight 68.4 kg, rate −0.5 %/week. The weekly change is 0.342 kg, so the deficit is 376 kcal/day and the target is **1752 kcal**.

### Weekly distribution

| Mode | Behavior |
|---|---|
| **Even** | Every day gets the average. |
| **Shift** | The chosen "high" days get 1.10× the low-day value, and the low-day value is solved so the weekly total is unchanged. |

```text
low  = avg · 7 / (n_low + 1.10 · n_high)
high = low · 1.10
```

If shifting would push low days below the floor, the week falls back to even.

Example: a 1752 kcal average with Friday and Saturday high gives five days at **1703** and two days at **1874**.

### Macros

**Protein** is set from body weight and the user's preference:

| Preference | g/kg |
|---|---|
| Low | 1.4 |
| Moderate (default) | 1.7 |
| High | 2.0 |
| Extra High | 2.4 |

**Fat and carbs** share the calories left after protein. Depending on the diet type, either fat or carbs is fixed, and the other macro takes whatever remains:

| Diet | Fixed | Remainder goes to |
|---|---|---|
| Balanced | fat = 30 % of total kcal | carbs |
| Low-fat | fat = 20 % of total kcal | carbs |
| Low-carb | carbs = 20 % of total kcal | fat |
| Keto | carbs = 30 g | fat |

If protein alone uses up the budget, fat and carbs are both zero.

Example: 1752 kcal at 68.4 kg, Moderate, Balanced gives **116 P / 58 F / 190 C**.

---

## 5. Weekly check-in (`openmacro-core::checkin`)

Targets are recomputed once a week rather than every day, so the plan stays stable.

**Schedule.** The user picks a check-in weekday (Monday by default).
- Before the first check-in, the next one is due on the next check-in weekday (today if it's that day).
- Afterwards, it's due on the first check-in weekday at least **4 days** after the last check-in.
- So checking in early or late moves the next check-in back onto the chosen weekday. Gaps stay between 4 and 10 days, centred on a week.
- An early check-in is always allowed.

**Review.** Before the update, the user reviews the days since the last check-in (at most 14, up to yesterday, never before the first logged day). Two kinds of day are flagged:
- **Nothing logged:** the estimator ignores these unless the user marks them fasted. A fasted day counts as 0 kcal.
- **Logged below 50 % of that day's target:** probably only partly logged. If the user marks them partial, the estimator ignores them.

Days already marked partial or fasted aren't shown again.

**Update.** A check-in stores the current expenditure estimate and trend weight. Until the next check-in:
- Targets are derived (§4) from those stored values, not the live estimate.
- Changing the goal or program still applies straight away, using the stored values.
- Before the first check-in, targets follow the live estimate.

The update step shows the old and new expenditure and average target. Once confirmed, the new targets become active immediately.

Planned:
- program styles: **Collaborative** keeps the user's protein and carb/fat preference and recomputes calories; **Manual** changes nothing;
- a logging-break explanation when the estimate is holding;
- switching individual modules off.

For a **Maintain** goal, the rate is 0 while the trend weight stays within ±1.5 % of the goal weight. Outside that band, a gentle ±0.25 %/week correction applies until the trend is back inside.
