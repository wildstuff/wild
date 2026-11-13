# Domain Knowledge — Energy Balance

## The core equation

Body weight change is driven, over time, by energy balance:

```
net balance = calories IN − calories OUT
```

- **Net < 0 (deficit)** → the body draws on stores → weight tends down.
  ~7,700 kcal ≈ 1 kg of body fat, so a steady ~550 kcal/day deficit ≈ ~0.5 kg/week.
- **Net ≈ 0 (maintenance)** → weight holds.
- **Net > 0 (surplus)** → weight tends up (muscle if training + protein are
  there, fat otherwise).

This tribe makes that ledger concrete and daily. It does NOT moralise food —
it counts energy and shows the trend.

## Calories IN — the food side

- A food's energy is stored per 100 g (`food.kcal_per_100g`); a logged entry's
  calories are `kcal_per_100g × grams ÷ 100`, computed at log time.
- **Macros** matter for quality, not just quantity: protein 4 kcal/g, carbs
  4 kcal/g, fat 9 kcal/g (the Atwater factors — `@kcal_from_macros` cross-checks
  a food's stated kcal against its macros, catching bad web-looked-up data).
- **Protein target** for an active person: ~1.6–2.2 g per kg body weight, to
  preserve muscle in a deficit.

## Calories OUT — the activity side

- Total daily expenditure (**TDEE**) = BMR + activity + the thermic effect of
  food. This tribe tracks the *logged exercise* portion explicitly; BMR is the
  large baseline the chief estimates.
- **BMR** (basal metabolic rate), Mifflin–St Jeor:
  `10×kg + 6.25×cm − 5×age + 5 (male) / −161 (female)`.
- An exercise's burn is stored per minute (`exercise.kcal_per_min`, ~70 kg
  reference); a logged workout's burn is `kcal_per_min × duration_min`. Scale
  by body weight for a heavier/lighter client.
- **MET** (metabolic equivalent) is the intensity factor: kcal/min ≈
  `MET × 3.5 × kg ÷ 200`.

## The numbers that matter (chief-computed)

The `energy_balance` view gives the rolling 7-day net directly. These need
averages / per-day breakdown the view DSL can't express yet, so compute them
from the records + the `daily_intake` / `daily_burn` composites:

| Number | How | Healthy signal |
|--------|-----|----------------|
| Daily net | `daily_intake.total_kcal_in − daily_burn.total_kcal_out` for the day | within ±10% of goal |
| 7-day average net | mean of the daily nets (or the `energy_balance` view ÷ 7) | tracks the goal |
| Logging streak | consecutive days with ≥1 `food_log` entry | trending up |
| Protein ratio | Σ protein from logged foods ÷ target | ≥ 1.0 |
| Active days/week | days with a `workout_log` entry | ≥ 3 |

## What is off-mission

Medical diagnoses, lab values, prescription diets, financial or business data.
Decline clearly, name the reason, redirect to food/activity logging. A one-off
"how many calories in an apple?" is answered directly (use `web-search` if
unsure) — no new tribe needed.
