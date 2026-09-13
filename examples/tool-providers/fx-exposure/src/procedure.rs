//! The fitted model — ADR-0202 D7's "inspectable calculation".
//!
//! One volatility per currency, and nothing else. The EUR value of a foreign
//! receivable is `amount / rate`, which is arithmetic and needs no model; what
//! is LEARNED is how far that rate typically moves before the money arrives.
//! So the answer is a band, not a number.
//!
//! **This is the Procedure whose answer depends on something outside itself.**
//! Its two siblings answer from their inputs alone — same inputs, same answer,
//! for ever. This one is anchored to a market rate that came from somewhere
//! else, on a date, and the answer is only reproducible if BOTH the model
//! version and that rate snapshot are recorded. That is the second axis on the
//! audit card, and the reason `rate_date` rides back out with every answer.
//!
//! The determinism profile is what forces the shape. ADR-0202 D10.3's import
//! allow-list carries no `wasi:sockets`, no `wasi:http` and no `wild:http`, so
//! a Procedure CANNOT fetch its own rates — the impure half has to be a
//! connector and the rate has to arrive as a field. That constraint reads like
//! an inconvenience and is the feature: a model that fetched its own inputs
//! could answer differently on two runs with nothing recorded to explain it.
//!
//! `tests::the_baked_parameters_are_the_fit_of_the_shipped_extract` re-derives
//! every volatility from `training/eurofx-2019-2026.csv`, and
//! `tests::the_manifest_holdout_score_is_the_measured_one` recomputes the
//! coverage the card claims — together with the flat-band baseline it is
//! claimed against.

/// The manifest version this artifact implements.
pub const MODEL_VERSION: &str = "v1";

/// One currency's fitted daily volatility of the log rate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurrencyFit {
    /// ISO 4217 code, quoted the ECB way: units of this currency per ONE euro.
    pub code: &'static str,
    /// Standard deviation of the daily log return over the training window.
    pub daily_sigma: f64,
    /// Daily returns the fit stood on — on the card because it is what makes
    /// the number believable, and because a currency added later will not have
    /// this many.
    pub n: u32,
}

/// The fitted book, ordered by code — the order the audit card lists.
pub const CURRENCIES: [CurrencyFit; 3] = [
    CurrencyFit { code: "CHF", daily_sigma: 0.002192, n: 1499 },
    CurrencyFit { code: "GBP", daily_sigma: 0.003453, n: 1499 },
    CurrencyFit { code: "USD", daily_sigma: 0.004512, n: 1499 },
];

/// Two-sided 80 % z-score.
///
/// 80 % rather than 95 % is a JUDGEMENT and sits on the card as one. A 95 %
/// band on a 30-day dollar move is so wide that a treasurer cannot plan with
/// it, and a band nobody acts on is a band nobody checks. 80 % means "one
/// month in five will land outside this", which is a sentence an accountant
/// can accept or reject.
pub const BAND_Z: f64 = 1.2816;

/// Trading days per calendar day. Rates are quoted on business days, so a
/// 30-calendar-day horizon is 21 quoted moves, not 30. Getting this wrong
/// inflates every band by ~19 % and the coverage with it.
pub const TRADING_DAY_RATIO: f64 = 5.0 / 7.0;

/// What the Procedure answers with.
#[derive(Debug, Clone, PartialEq)]
pub struct Valuation {
    /// The receivable's EUR value at the anchoring rate, two decimals, as a
    /// string. Decimals cross this boundary as text by house rule.
    pub expected_eur: String,
    /// The low and high ends of the 80 % band, same convention.
    pub band_low_eur: String,
    pub band_high_eur: String,
    /// Half-width as a percentage of the expected value, one decimal — the
    /// number a treasurer actually reads ("±2,7 %").
    pub band_pct: String,
    /// THE SECOND AXIS, echoed back deliberately. The answer is only
    /// reproducible against the snapshot it was anchored to, so the row that
    /// stores it has to carry that date whether or not anyone asked for it.
    pub rate_date: String,
    /// `fitted` when a volatility carried the band, `no_fx_risk` for a euro
    /// receivable, which has none.
    pub basis: String,
    pub procedure_version: String,
}

pub const BASIS_FITTED: &str = "fitted";
pub const BASIS_NO_FX_RISK: &str = "no_fx_risk";

/// Why a valuation could not be produced.
#[derive(Debug, Clone, PartialEq)]
pub enum ValuationError {
    /// A currency the fit never saw.
    ///
    /// REFUSED, not answered — and deliberately unlike `payment-delay`, whose
    /// unknown debtor gets the pooled mean under `no_history`. There, a pooled
    /// answer is a real statement about a real population. Here there is no
    /// pooled volatility worth having: the Swiss franc and the dollar differ
    /// by a factor of two, so any average is wrong for both, and a band that
    /// is wrong is worse than no band because it will be planned against.
    UnknownCurrency(String),
    /// The rate was absent, zero or negative. A rate of zero divides the book
    /// by nothing; a negative one is not a quote.
    BadRate(String),
    /// The horizon was negative. Zero is legal — it means "value it today".
    BadHorizon(i64),
    /// `rate_date` was not an ISO `YYYY-MM-DD` date. Refused rather than
    /// carried through blank, because the date IS the second audit axis: an
    /// answer anchored to an unreadable snapshot cannot be re-checked, and
    /// storing it anyway would leave a row that looks complete.
    BadRateDate(String),
}

impl std::fmt::Display for ValuationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownCurrency(c) => write!(
                f,
                "no fitted volatility for {c} — the Procedure covers {}",
                CURRENCIES
                    .iter()
                    .map(|c| c.code)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::BadRate(r) => write!(f, "rate_per_eur is not a usable quote: {r}"),
            Self::BadHorizon(d) => write!(f, "days_to_settlement is negative: {d}"),
            Self::BadRateDate(s) => write!(f, "rate_date is not an ISO date (YYYY-MM-DD): {s}"),
        }
    }
}

/// The euro code, which needs no conversion and carries no risk.
pub const EUR: &str = "EUR";

pub fn currency_fit(code: &str) -> Option<&'static CurrencyFit> {
    CURRENCIES.iter().find(|c| c.code == code)
}

/// Trading days spanned by a calendar-day horizon.
pub fn trading_days(calendar_days: i64) -> f64 {
    (calendar_days as f64 * TRADING_DAY_RATIO).round()
}

/// The band half-width in LOG space for one currency over a horizon.
pub fn log_band(fit: &CurrencyFit, calendar_days: i64) -> f64 {
    BAND_Z * fit.daily_sigma * trading_days(calendar_days).sqrt()
}

/// Strict ISO `YYYY-MM-DD` shape check. Deliberately shape-only: the component
/// has no clock and no calendar authority, so it can say "that is not a date"
/// and must not say "that date is wrong".
pub fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let num = |r: std::ops::Range<usize>| s[r].bytes().all(|c| c.is_ascii_digit());
    if !(num(0..4) && num(5..7) && num(8..10)) {
        return false;
    }
    let m: u32 = s[5..7].parse().unwrap_or(0);
    let d: u32 = s[8..10].parse().unwrap_or(0);
    (1..=12).contains(&m) && (1..=31).contains(&d)
}

/// Euro, two decimals, rounded half away from zero.
pub fn money_2dp(v: f64) -> String {
    let cents = (v * 100.0).round() as i64;
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.abs();
    format!("{sign}{}.{:02}", abs / 100, abs % 100)
}

/// One decimal, rounded half away from zero.
pub fn one_dp(v: f64) -> String {
    let tenths = (v * 10.0).round() as i64;
    let sign = if tenths < 0 { "-" } else { "" };
    let abs = tenths.abs();
    format!("{sign}{}.{}", abs / 10, abs % 10)
}

/// The valuation itself.
///
/// `rate_per_eur` is the ECB convention — units of `currency` per ONE euro —
/// so the EUR value is `amount / rate` and a RISING rate means the euro
/// strengthened and the receivable is worth LESS. The band therefore inverts:
/// the high end of the rate is the low end of the euro amount.
pub fn value(
    currency: &str,
    amount_foreign: f64,
    rate_per_eur: f64,
    rate_date: &str,
    days_to_settlement: i64,
) -> Result<Valuation, ValuationError> {
    if !is_iso_date(rate_date) {
        return Err(ValuationError::BadRateDate(rate_date.to_string()));
    }
    if days_to_settlement < 0 {
        return Err(ValuationError::BadHorizon(days_to_settlement));
    }

    if currency == EUR {
        // No conversion, no risk, and the band is a point. Answering this
        // rather than refusing keeps a mixed book from needing two code paths
        // on the caller's side.
        let eur = money_2dp(amount_foreign);
        return Ok(Valuation {
            expected_eur: eur.clone(),
            band_low_eur: eur.clone(),
            band_high_eur: eur,
            band_pct: one_dp(0.0),
            rate_date: rate_date.to_string(),
            basis: BASIS_NO_FX_RISK.to_string(),
            procedure_version: MODEL_VERSION.to_string(),
        });
    }

    let fit = currency_fit(currency)
        .ok_or_else(|| ValuationError::UnknownCurrency(currency.to_string()))?;
    if !rate_per_eur.is_finite() || rate_per_eur <= 0.0 {
        return Err(ValuationError::BadRate(rate_per_eur.to_string()));
    }

    let expected = amount_foreign / rate_per_eur;
    let band = log_band(fit, days_to_settlement);
    // The rate's high end gives the euro amount's low end.
    let low = amount_foreign / (rate_per_eur * band.exp());
    let high = amount_foreign / (rate_per_eur * (-band).exp());

    Ok(Valuation {
        expected_eur: money_2dp(expected),
        band_low_eur: money_2dp(low),
        band_high_eur: money_2dp(high),
        band_pct: one_dp(100.0 * (band.exp() - 1.0)),
        rate_date: rate_date.to_string(),
        basis: BASIS_FITTED.to_string(),
        procedure_version: MODEL_VERSION.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXTRACT: &str = include_str!("../training/eurofx-2019-2026.csv");
    const CARD: &str = include_str!("../procedure.yaml");

    /// Business days in the training window, per currency.
    const TRAIN_DAYS: usize = 1500;
    /// The horizon the card's coverage is scored at, in trading days —
    /// `round(30 × 5/7)`, the same step `fit.py` uses.
    const SCORE_STEP: usize = 21;

    /// `code -> the rate path, in date order`.
    fn series() -> std::collections::BTreeMap<String, Vec<f64>> {
        let mut out: std::collections::BTreeMap<String, Vec<f64>> = Default::default();
        for line in EXTRACT.lines().skip(1).filter(|l| !l.trim().is_empty()) {
            let mut it = line.split(',');
            let _date = it.next().unwrap();
            let ccy = it.next().unwrap().to_string();
            let rate: f64 = it.next().unwrap().parse().unwrap();
            out.entry(ccy).or_default().push(rate);
        }
        out
    }

    fn stdev(xs: &[f64]) -> f64 {
        let m = xs.iter().sum::<f64>() / xs.len() as f64;
        (xs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (xs.len() - 1) as f64).sqrt()
    }

    #[test]
    fn the_extract_is_the_window_the_card_names() {
        let s = series();
        assert_eq!(s.len(), 3, "three currencies");
        for (ccy, path) in &s {
            assert_eq!(path.len(), 1970, "{ccy}: 1970 quoted business days");
        }
        let first = EXTRACT.lines().nth(1).unwrap();
        assert!(first.starts_with("2019-01-02,"), "the epoch leads: {first}");
        let last = EXTRACT.lines().last().unwrap();
        assert!(
            last.starts_with("2026-07-21,"),
            "the extract stops before the tribe's own cut-off: {last}"
        );
    }

    /// D7.2 — an auditor must be able to re-check the calculation.
    #[test]
    fn the_baked_parameters_are_the_fit_of_the_shipped_extract() {
        let s = series();
        for fit in &CURRENCIES {
            let path = &s[fit.code][..TRAIN_DAYS];
            let rets: Vec<f64> = path.windows(2).map(|w| (w[1] / w[0]).ln()).collect();
            assert_eq!(rets.len() as u32, fit.n, "{}: return count", fit.code);
            let sd = stdev(&rets);
            assert!(
                (sd - fit.daily_sigma).abs() < 5e-7,
                "{}: daily_sigma drifted — fitted {sd:.6}, baked {:.6}",
                fit.code,
                fit.daily_sigma
            );
        }
    }

    /// The card's coverage is a measurement, and so is the flat-band baseline
    /// it is claimed against.
    ///
    /// The AGGREGATE is the headline and it is deliberately unimpressive:
    /// 0.7595 against 0.7491 for a rule that adds two percent to everything.
    /// Read alone it says the Procedure is barely worth having. The per-currency
    /// split is what it actually says — and that gap is the finding, not a
    /// presentational choice.
    #[test]
    fn the_manifest_holdout_score_is_the_measured_one() {
        let s = series();
        let flat = 1.02f64.ln();
        let (mut hits, mut base_hits, mut total) = (0usize, 0usize, 0usize);

        for fit in &CURRENCIES {
            let path = &s[fit.code][TRAIN_DAYS..];
            let band = BAND_Z * fit.daily_sigma * (SCORE_STEP as f64).sqrt();
            let (mut h, mut b, mut n) = (0usize, 0usize, 0usize);
            for i in 0..path.len() - SCORE_STEP {
                let mv = (path[i + SCORE_STEP] / path[i]).ln().abs();
                n += 1;
                h += usize::from(mv <= band);
                b += usize::from(mv <= flat);
            }
            hits += h;
            base_hits += b;
            total += n;

            // The per-currency claim the README makes, currency by currency.
            let (cov, base) = (h as f64 / n as f64, b as f64 / n as f64);
            let expected: (f64, f64) = match fit.code {
                "CHF" => (0.7884, 0.9577),
                "GBP" => (0.7817, 0.7751),
                "USD" => (0.7082, 0.5145),
                other => panic!("unscored currency {other}"),
            };
            assert!(
                (cov - expected.0).abs() < 5e-4 && (base - expected.1).abs() < 5e-4,
                "{}: measured coverage {cov:.4} / baseline {base:.4}, documented {expected:?}",
                fit.code
            );
        }

        let coverage = hits as f64 / total as f64;
        let base_coverage = base_hits as f64 / total as f64;

        // `model.yaml` claims holdout.score = 0.7595 (metric: band_coverage_80).
        assert!(
            (coverage - 0.7595).abs() < 5e-4,
            "the manifest claims band_coverage_80 0.7595 but the extract measures {coverage:.4}"
        );
        assert!(
            (base_coverage - 0.7491).abs() < 5e-4,
            "the flat-band baseline measures {base_coverage:.4}, not the claimed 0.7491"
        );

        // THE claim, and it is not "the model scores higher". Aggregate, the
        // two are indistinguishable; per currency the flat band is absurdly
        // wide for the franc and far too narrow for the dollar, and only the
        // fitted one is within reach of its target everywhere.
        assert!(
            (coverage - base_coverage).abs() < 0.02,
            "the aggregate is supposed to be unable to tell them apart — if it \
             can, the README's whole argument for reading per currency is gone"
        );
    }

    /// The card is what an auditor is handed, and it restates every fitted
    /// number by hand.
    #[test]
    fn the_audit_card_states_what_the_component_uses() {
        assert!(CARD.contains(&format!("version: {MODEL_VERSION}")));
        assert!(CARD.contains("snapshot: eurofx-2019-2026"));
        assert!(CARD.contains(&format!("band_z: {BAND_Z}")));
        for fit in &CURRENCIES {
            let block = card_block(fit.code);
            for (k, want) in [
                ("daily_sigma", format!("{:.6}", fit.daily_sigma)),
                ("n", format!("{}", fit.n)),
            ] {
                let line = format!("{k}: {want}");
                assert!(
                    block.iter().any(|l| l.trim() == line),
                    "{}: the card does not state `{line}` — block reads {block:?}",
                    fit.code
                );
            }
        }
    }

    fn card_block(code: &str) -> Vec<&'static str> {
        let head = format!("{code}:");
        let mut out = Vec::new();
        let mut inside = false;
        for line in CARD.lines() {
            if line.trim() == head {
                inside = true;
                continue;
            }
            if inside {
                if line.trim().is_empty() || !line.starts_with("      ") {
                    break;
                }
                out.push(line);
            }
        }
        assert!(!out.is_empty(), "{code} is missing from the card");
        out
    }

    /// The band is the whole answer, so its shape is worth pinning: wider with
    /// the horizon, wider for the more volatile currency, and never
    /// symmetric in euro terms — the rate moves multiplicatively.
    #[test]
    fn the_band_widens_with_horizon_and_with_volatility() {
        let chf = value("CHF", 10_000.0, 0.9420, "2026-07-21", 30).unwrap();
        let usd = value("USD", 10_000.0, 1.0850, "2026-07-21", 30).unwrap();
        let usd_far = value("USD", 10_000.0, 1.0850, "2026-07-21", 120).unwrap();

        let pct = |v: &Valuation| v.band_pct.parse::<f64>().unwrap();
        assert!(
            pct(&chf) < pct(&usd),
            "the franc must price tighter than the dollar: {} vs {}",
            chf.band_pct,
            usd.band_pct
        );
        assert!(
            pct(&usd) < pct(&usd_far),
            "four months out must be wider than one: {} vs {}",
            usd.band_pct,
            usd_far.band_pct
        );

        // Multiplicative, so the euro band is not symmetric about the mean.
        let (e, lo, hi) = (
            usd.expected_eur.parse::<f64>().unwrap(),
            usd.band_low_eur.parse::<f64>().unwrap(),
            usd.band_high_eur.parse::<f64>().unwrap(),
        );
        assert!(lo < e && e < hi);
        assert!(
            (hi - e) > (e - lo),
            "an inverted multiplicative band is wider on the upside: {lo} {e} {hi}"
        );
    }

    /// The second axis has to come back out, or the row that stores the answer
    /// cannot say what it was anchored to.
    #[test]
    fn the_answer_carries_the_snapshot_it_was_anchored_to() {
        let v = value("USD", 10_000.0, 1.0850, "2026-07-21", 30).unwrap();
        assert_eq!(v.rate_date, "2026-07-21");
        assert_eq!(v.basis, BASIS_FITTED);
        assert_eq!(v.procedure_version, "v1");

        // Same model, same invoice, a different snapshot — a different answer.
        // If these ever matched, the second audit axis would be decoration.
        let other = value("USD", 10_000.0, 1.1200, "2026-06-21", 30).unwrap();
        assert_ne!(v.expected_eur, other.expected_eur);
        assert_eq!(
            v.band_pct, other.band_pct,
            "the BAND is the model's and does not move with the anchor"
        );
    }

    #[test]
    fn a_euro_receivable_is_itself_and_says_so() {
        let v = value(EUR, 8_800.0, 1.0, "2026-07-21", 30).unwrap();
        assert_eq!(v.expected_eur, "8800.00");
        assert_eq!(v.band_low_eur, "8800.00");
        assert_eq!(v.band_high_eur, "8800.00");
        assert_eq!(v.basis, BASIS_NO_FX_RISK);
    }

    /// An unknown currency is REFUSED. The sibling `payment-delay` answers its
    /// unknown debtor with the pooled mean; here there is no pooled volatility
    /// worth having, and a band that is wrong is worse than no band because it
    /// will be planned against.
    #[test]
    fn an_unfitted_currency_is_refused_rather_than_averaged() {
        let e = value("JPY", 1_000_000.0, 168.0, "2026-07-21", 30).unwrap_err();
        assert!(matches!(e, ValuationError::UnknownCurrency(_)));
        assert!(e.to_string().contains("CHF, GBP, USD"), "{e}");
    }

    #[test]
    fn a_bad_anchor_is_refused() {
        assert!(matches!(
            value("USD", 100.0, 1.08, "21.07.2026", 30),
            Err(ValuationError::BadRateDate(_))
        ));
        assert!(matches!(
            value("USD", 100.0, 0.0, "2026-07-21", 30),
            Err(ValuationError::BadRate(_))
        ));
        assert!(matches!(
            value("USD", 100.0, -1.08, "2026-07-21", 30),
            Err(ValuationError::BadRate(_))
        ));
        assert!(matches!(
            value("USD", 100.0, 1.08, "2026-07-21", -1),
            Err(ValuationError::BadHorizon(_))
        ));
    }

    #[test]
    fn a_zero_horizon_values_it_today_with_no_band() {
        let v = value("USD", 10_850.0, 1.0850, "2026-07-21", 0).unwrap();
        assert_eq!(v.expected_eur, "10000.00");
        assert_eq!(v.band_pct, "0.0");
        assert_eq!(v.basis, BASIS_FITTED);
    }
}
