//! The fitted model — ADR-0202 D7's "inspectable calculation".
//!
//! Everything the forecast needs is in this file as a constant: a baseline
//! and 52 weekly factors. There is no training code here and no data read at
//! run time, which is exactly the property the audit card promises — given
//! the same inputs, v4 answers the same number today and in three years.
//!
//! The parameters are not decorative. `tests::the_baked_parameters_are_the
//! _fit_of_the_shipped_extract` re-derives them from
//! `training/inflows-2023-2026.csv` and fails if a single factor drifts, and
//! `tests::the_manifest_holdout_score_is_the_measured_one` recomputes the
//! MAPE the manifest claims. D7.2 asks that an auditor be able to re-check a
//! Procedure with a spreadsheet; these two tests are that spreadsheet.

/// The manifest version this artifact implements. Stamped onto every
/// estimate so a later redeploy changes future values without rewriting
/// the past (ADR-0202 D3).
pub const MODEL_VERSION: &str = "v4";

/// Weekly buckets are counted from this Monday. The anchor is part of the
/// fitted model — it is what makes `week_of_year` reproducible for any date
/// rather than dependent on a calendar convention.
pub const EPOCH: (i32, u32, u32) = (2023, 1, 2);

/// Mean weekly inflow over the training window, in euro.
pub const BASELINE_EUR: f64 = 42068.98;

/// Multiplicative seasonal factor per weekly bucket (1..=52), fitted as
/// `mean(bucket) / baseline` over the training window.
pub const FACTORS: [f64; 52] = [
    1.139438, 1.155849, 1.173192, 1.114195, 1.103440, 1.072070, 1.083286, 1.071456, 1.042828,
    1.010776, 1.000225, 1.012079, 0.959928, 0.908860, 0.920973, 0.925069, 0.885404, 0.872630,
    0.819894, 0.853340, 0.827174, 0.833893, 0.830658, 0.838042, 0.828616, 0.802870, 0.818511,
    0.817202, 0.856734, 0.853330, 0.880912, 0.906944, 0.912238, 0.934974, 0.955776, 1.018011,
    1.021590, 1.009439, 1.001600, 1.050684, 1.077873, 1.127362, 1.124998, 1.156639, 1.126004,
    1.168409, 1.147628, 1.175840, 1.194009, 1.193881, 1.168714, 1.214514,
];

/// What the Procedure answers with.
#[derive(Debug, Clone, PartialEq)]
pub struct Forecast {
    /// The date the estimate is ABOUT — ADR-0202 D3's valid-time anchor,
    /// never the run date.
    pub target_date: String,
    /// Expected inflow, euro, two decimals, as a string. Decimals cross
    /// this boundary as text by house rule: a JSON number would round-trip
    /// through a float and stop being the number that was written.
    pub expected_inflow: String,
    /// Which weekly bucket answered — the one number that lets an operator
    /// find the row in the parameter dump.
    pub week_bucket: u32,
    pub procedure_version: String,
}

/// Why a forecast could not be produced. Deliberately narrow: every variant
/// is something the caller can fix.
#[derive(Debug, Clone, PartialEq)]
pub enum ForecastError {
    /// `as_of` was not an ISO `YYYY-MM-DD` date.
    BadDate(String),
    /// The horizon was absent, negative, or past the year this model was
    /// fitted to describe.
    BadHorizon(String),
}

impl std::fmt::Display for ForecastError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadDate(s) => write!(f, "as_of is not an ISO date (YYYY-MM-DD): {s}"),
            Self::BadHorizon(s) => write!(f, "horizon_days out of range: {s}"),
        }
    }
}

/// The furthest ahead this model will answer. A seasonal fit over one year
/// of buckets says nothing useful beyond that, and answering anyway would be
/// the kind of confident wrongness D7 exists to prevent.
pub const MAX_HORIZON_DAYS: i64 = 365;

// ── civil-date arithmetic (Howard Hinnant's algorithm, no dependencies) ──

/// Days since 1970-01-01 for a proleptic-Gregorian civil date.
pub fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = ((m + 9) % 12) as i64; // Mar = 0
    let doy = (153 * mp + 2) / 5 + d as i64 - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

/// Inverse of [`days_from_civil`].
pub fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

/// Parse an ISO `YYYY-MM-DD` date. Strict on shape — a forecast keyed off a
/// misread date is worse than a refusal.
pub fn parse_iso_date(s: &str) -> Option<(i32, u32, u32)> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: i32 = s[0..4].parse().ok()?;
    let m: u32 = s[5..7].parse().ok()?;
    let d: u32 = s[8..10].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Reject impossible days (e.g. 2023-02-30) by round-tripping.
    let days = days_from_civil(y, m, d);
    (civil_from_days(days) == (y, m, d)).then_some((y, m, d))
}

pub fn format_iso_date(y: i32, m: u32, d: u32) -> String {
    format!("{y:04}-{m:02}-{d:02}")
}

/// The weekly bucket (1..=52) a date falls in, counted from [`EPOCH`].
pub fn week_bucket(y: i32, m: u32, d: u32) -> u32 {
    let epoch = days_from_civil(EPOCH.0, EPOCH.1, EPOCH.2);
    let weeks = (days_from_civil(y, m, d) - epoch).div_euclid(7);
    (weeks.rem_euclid(52) + 1) as u32
}

/// Euro, two decimals, rounded half away from zero (which is commercial
/// half-up for the non-negative amounts a forecast produces).
///
/// Note the limit this cannot escape: `v` is an `f64`, so a decimal that is
/// not representable in binary (`1.005` is really 1.00499…) rounds by what
/// the bits say, not by what the decimal literal looks like. That is fine
/// here — the model claims cents, not sub-cent exactness — but it is why
/// the store keeps money as text and why this returns a string.
pub fn money_2dp(v: f64) -> String {
    let cents = (v * 100.0).round() as i64;
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

/// The forecast itself.
pub fn forecast(as_of: &str, horizon_days: i64) -> Result<Forecast, ForecastError> {
    let (y, m, d) =
        parse_iso_date(as_of).ok_or_else(|| ForecastError::BadDate(as_of.to_string()))?;
    if horizon_days < 0 || horizon_days > MAX_HORIZON_DAYS {
        return Err(ForecastError::BadHorizon(format!(
            "{horizon_days} (expected 0..={MAX_HORIZON_DAYS})"
        )));
    }
    let target = days_from_civil(y, m, d) + horizon_days;
    let (ty, tm, td) = civil_from_days(target);
    let bucket = week_bucket(ty, tm, td);
    let expected = BASELINE_EUR * FACTORS[(bucket - 1) as usize];
    Ok(Forecast {
        target_date: format_iso_date(ty, tm, td),
        expected_inflow: money_2dp(expected),
        week_bucket: bucket,
        procedure_version: MODEL_VERSION.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped training extract, read at test time so the parameters
    /// above are checkable rather than asserted.
    const EXTRACT: &str = include_str!("../training/inflows-2023-2026.csv");

    /// The window the manifest names as `trained_on.as_of`.
    const TRAIN_WEEKS: usize = 156;

    fn rows() -> Vec<(String, u32, f64)> {
        EXTRACT
            .lines()
            .skip(1)
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let mut it = l.split(',');
                let date = it.next().unwrap().to_string();
                let week: u32 = it.next().unwrap().parse().unwrap();
                let amount: f64 = it.next().unwrap().parse().unwrap();
                (date, week, amount)
            })
            .collect()
    }

    #[test]
    fn the_extract_is_four_years_of_weekly_rows() {
        let r = rows();
        assert_eq!(r.len(), 208, "4 years of weekly buckets");
        assert_eq!(r[0].0, "2023-01-02", "the epoch Monday leads the extract");
        assert_eq!(
            r[TRAIN_WEEKS - 1].0,
            "2025-12-22",
            "training window ends here"
        );
    }

    /// D7.2 — an auditor must be able to re-check the calculation. This IS
    /// that re-check: the baked constants are re-derived from the extract.
    #[test]
    fn the_baked_parameters_are_the_fit_of_the_shipped_extract() {
        let r = rows();
        let train = &r[..TRAIN_WEEKS];

        let baseline: f64 = train.iter().map(|(_, _, a)| a).sum::<f64>() / train.len() as f64;
        assert!(
            (baseline - BASELINE_EUR).abs() < 0.005,
            "BASELINE_EUR drifted from the extract: fitted {baseline:.2}, baked {BASELINE_EUR:.2}"
        );

        for bucket in 1..=52u32 {
            let vals: Vec<f64> = train
                .iter()
                .filter(|(_, w, _)| *w == bucket)
                .map(|(_, _, a)| *a)
                .collect();
            assert!(!vals.is_empty(), "bucket {bucket} has no training sample");
            let fitted = (vals.iter().sum::<f64>() / vals.len() as f64) / baseline;
            let baked = FACTORS[(bucket - 1) as usize];
            assert!(
                (fitted - baked).abs() < 1e-6,
                "factor for bucket {bucket} drifted: fitted {fitted:.6}, baked {baked:.6}"
            );
        }
    }

    /// The holdout score on the audit card is a measurement, not a claim.
    #[test]
    fn the_manifest_holdout_score_is_the_measured_one() {
        let r = rows();
        let holdout = &r[TRAIN_WEEKS..];
        assert_eq!(holdout.len(), 52, "the holdout is the third year");

        let errs: Vec<f64> = holdout
            .iter()
            .map(|(_, w, actual)| {
                let pred = BASELINE_EUR * FACTORS[(*w - 1) as usize];
                (pred - actual).abs() / actual
            })
            .collect();
        let mape = errs.iter().sum::<f64>() / errs.len() as f64;

        // `procedure.yaml` claims holdout.score = 0.0353 (metric: mape).
        assert!(
            (mape - 0.0353).abs() < 0.0005,
            "the manifest claims mape 0.0353 but the extract measures {mape:.4}"
        );
    }

    /// The bucket the model uses has to be the bucket the extract was
    /// labelled with, or the parameters describe a different calendar.
    #[test]
    fn the_week_bucket_agrees_with_the_extract_labels() {
        for (date, week, _) in rows() {
            let (y, m, d) = parse_iso_date(&date).expect("extract date parses");
            assert_eq!(week_bucket(y, m, d), week, "bucket disagrees for {date}");
        }
    }

    #[test]
    fn civil_date_arithmetic_round_trips_across_leap_years() {
        for (y, m, d) in [
            (2023, 1, 2),
            (2024, 2, 29),
            (2025, 12, 31),
            (2000, 2, 29),
            (1970, 1, 1),
        ] {
            assert_eq!(civil_from_days(days_from_civil(y, m, d)), (y, m, d));
        }
    }

    #[test]
    fn an_impossible_date_is_refused_rather_than_normalised() {
        assert!(parse_iso_date("2023-02-30").is_none());
        assert!(parse_iso_date("2023-13-01").is_none());
        assert!(parse_iso_date("2023-1-01").is_none(), "shape is strict");
        assert!(parse_iso_date("").is_none());
        assert!(parse_iso_date("2023-02-28").is_some());
    }

    #[test]
    fn the_horizon_is_bounded_at_both_ends() {
        assert!(matches!(
            forecast("2025-01-06", -1),
            Err(ForecastError::BadHorizon(_))
        ));
        assert!(matches!(
            forecast("2025-01-06", MAX_HORIZON_DAYS + 1),
            Err(ForecastError::BadHorizon(_))
        ));
        assert!(forecast("2025-01-06", MAX_HORIZON_DAYS).is_ok());
        assert!(forecast("2025-01-06", 0).is_ok());
    }

    /// The anchor is the estimate's TARGET date (D3), never the run date —
    /// the difference between a chart that answers and one that refuses.
    #[test]
    fn the_target_date_is_the_horizon_ahead_not_the_run_date() {
        let f = forecast("2025-01-06", 30).expect("forecast");
        assert_eq!(f.target_date, "2025-02-05");
        assert_ne!(f.target_date, "2025-01-06");
        assert_eq!(f.procedure_version, "v4");
    }

    #[test]
    fn the_same_inputs_answer_the_same_number() {
        let a = forecast("2025-03-17", 30).expect("a");
        let b = forecast("2025-03-17", 30).expect("b");
        assert_eq!(a, b, "a fitted model is a pure function of its inputs");
    }

    #[test]
    fn money_is_two_decimals_rounded_half_away_from_zero() {
        // 0.125 and 2.675 are chosen because they ARE representable at the
        // rounding boundary. `1.005` deliberately is not tested: it is
        // really 1.00499… in f64, so asserting half-up on it would be
        // asserting a property the input cannot exhibit.
        assert_eq!(money_2dp(0.125), "0.13");
        assert_eq!(money_2dp(42308.60), "42308.60");
        assert_eq!(money_2dp(0.0), "0.00");
        assert_eq!(money_2dp(12.3), "12.30");
    }

    /// A forecast is never negative, but a rounding helper that silently
    /// drops the sign is the kind of thing that survives until something
    /// else reuses it.
    #[test]
    fn money_keeps_the_sign_below_one_euro() {
        assert_eq!(money_2dp(-0.5), "-0.50");
        assert_eq!(money_2dp(-0.04), "-0.04");
        assert_eq!(money_2dp(-12.34), "-12.34");
    }

    /// The card and the code hold the SAME numbers twice, and until now
    /// nothing held them to each other.
    ///
    /// `the_baked_parameters_are_the_fit_of_the_shipped_extract` proves the
    /// constants are the fit; `the_manifest_holdout_score_is_the_measured_one`
    /// recomputes the MAPE. Both check the CODE. The manifest — the artifact
    /// an auditor is actually handed (ADR-0202 D7.2) — was agreeing by
    /// discipline: 52 hand-copied factors, and a refit touches every one of
    /// them. A card that disagrees with the component it describes is worse
    /// than no card, because it is the half the auditor trusts.
    #[test]
    fn the_audit_card_states_the_parameters_the_component_actually_uses() {
        const CARD: &str = include_str!("../procedure.yaml");

        let scalar = |key: &str| -> String {
            CARD.lines()
                .map(str::trim)
                .find_map(|l| l.strip_prefix(&format!("{key}:")))
                .unwrap_or_else(|| panic!("card states `{key}`"))
                .trim()
                .to_string()
        };

        assert_eq!(scalar("version"), MODEL_VERSION, "card version vs code");
        assert_eq!(
            scalar("baseline_eur").parse::<f64>().expect("numeric"),
            BASELINE_EUR,
            "card baseline vs code"
        );
        assert_eq!(
            scalar("snapshot"),
            "inflows-2023-2026",
            "the card must name the extract the tests re-derive from"
        );

        // Every one of the 52, by bucket — a single transposed pair is the
        // failure this exists for, and it would move exactly one week's
        // forecast.
        for (i, baked) in FACTORS.iter().enumerate() {
            let claimed: f64 = scalar(&format!("kw{}", i + 1))
                .parse()
                .expect("numeric factor");
            assert_eq!(
                claimed,
                *baked,
                "card kw{} disagrees with FACTORS[{i}]",
                i + 1
            );
        }
    }
}
