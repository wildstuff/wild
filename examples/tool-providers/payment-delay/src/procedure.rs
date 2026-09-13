//! The fitted model — ADR-0202 D7's "inspectable calculation".
//!
//! Everything the estimate needs is in this file as a constant: seven debtor
//! rows, a pooled mean and one shrinkage constant. There is no training code
//! here and no data read at run time, which is exactly the property the audit
//! card promises — given the same inputs, v1 answers the same day today and
//! in three years.
//!
//! **Only FITTED quantities are baked.** The number the Procedure actually
//! answers with — the shrunk delay — is computed here from `n`, the debtor's
//! own mean and [`POOLED_MEAN_DELAY_DAYS`], never stored a second time. A
//! model file and an audit card both carrying the shrunk value would be two
//! hand-maintained copies of one number, and the house rule against a
//! hand-mirrored surface applies to a Procedure like it does to anything
//! else. The card carries it because an accountant reads the card, and
//! `tests::the_audit_card_states_what_the_component_computes` holds the two
//! together.
//!
//! The parameters are not decorative either.
//! `tests::the_baked_parameters_are_the_fit_of_the_shipped_extract`
//! re-derives them from `training/settlements-2023-2026.csv` and fails if one
//! drifts; `tests::the_manifest_holdout_score_is_the_measured_one` recomputes
//! the MAE the manifest claims. D7.2 asks that an auditor be able to re-check
//! a Procedure with a spreadsheet; those tests are that spreadsheet.

/// The manifest version this artifact implements. Stamped onto every estimate
/// so a later redeploy changes future values without rewriting the past
/// (ADR-0202 D3).
pub const MODEL_VERSION: &str = "v1";

/// What one debtor's settlement history came out as. Every field is a
/// MEASUREMENT over the training window — nothing here is a choice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DebtorFit {
    pub partner_id: &'static str,
    /// Settled invoices this debtor contributed to the training window. It is
    /// on the card because it is what decides how much the debtor's own mean
    /// is believed — see [`SHRINKAGE_K`].
    pub n: u32,
    /// Mean days between due date and payment. Positive = pays late.
    pub own_mean_days: f64,
    /// Sample standard deviation of those delays. Carried into the answer,
    /// because a mean without a spread reads as confidence the fit does not
    /// have: BP-004's note says "irregular payment behaviour", and a
    /// component reporting only `+12 days` would hide exactly that.
    pub observed_stdev_days: f64,
}

/// The fitted book. Ordered by partner id, which is also how the audit card
/// lists them — an auditor comparing the two reads down two columns.
pub const DEBTORS: [DebtorFit; 7] = [
    DebtorFit { partner_id: "BP-001", n: 171, own_mean_days: 4.0819, observed_stdev_days: 1.7130 },
    DebtorFit { partner_id: "BP-002", n: 171, own_mean_days: -2.0702, observed_stdev_days: 1.2299 },
    DebtorFit { partner_id: "BP-003", n: 172, own_mean_days: 2.1047, observed_stdev_days: 1.7739 },
    DebtorFit { partner_id: "BP-004", n: 172, own_mean_days: 11.6279, observed_stdev_days: 4.7537 },
    DebtorFit { partner_id: "BP-005", n: 171, own_mean_days: 7.3743, observed_stdev_days: 2.9105 },
    DebtorFit { partner_id: "BP-006", n: 172, own_mean_days: -2.9884, observed_stdev_days: 1.1945 },
    DebtorFit { partner_id: "BP-007", n: 15, own_mean_days: 0.4000, observed_stdev_days: 2.1647 },
];

/// Mean delay over the whole training window — the value a thin history is
/// shrunk toward, and the answer for a partner the fit never saw.
pub const POOLED_MEAN_DELAY_DAYS: f64 = 3.3132;

/// Spread of the pooled delays. Much wider than any single debtor's (5.70
/// against 1.2–4.8), and that gap IS the model: most of the pooled spread is
/// variation BETWEEN debtors, which is the part a per-debtor mean removes.
pub const POOLED_STDEV_DAYS: f64 = 5.6998;

/// The shrinkage constant: a debtor's own mean carries weight `n / (n + K)`.
///
/// A JUDGEMENT, not a fit. K = 10 says "ten settled invoices is when I start
/// believing a debtor's own average" — a sentence an accountant can agree or
/// disagree with, which is why it is on the card as a parameter rather than
/// buried as a magic number.
pub const SHRINKAGE_K: f64 = 10.0;

/// Above this weight the answer is essentially the debtor's own mean and says
/// `own_history`; below it, `blended`. A REPORTING threshold only — moving it
/// changes what the row calls itself, never what it predicts.
pub const OWN_HISTORY_MIN_WEIGHT: f64 = 0.8;

/// How much of the answer is the debtor's own history. An open token: the set
/// of useful honesty labels is not closed, and a caller that meets an unknown
/// one should show it, not refuse it.
pub const BASIS_OWN_HISTORY: &str = "own_history";
pub const BASIS_BLENDED: &str = "blended";
pub const BASIS_NO_HISTORY: &str = "no_history";

/// What the Procedure answers with.
#[derive(Debug, Clone, PartialEq)]
pub struct DelayEstimate {
    /// When the invoice is expected to actually settle — the date the
    /// estimate is ABOUT, never the run date (ADR-0202 D3).
    pub expected_payment_date: String,
    /// Whole days after the due date. Negative = expected early.
    pub expected_delay_days: i64,
    /// The spread the estimate carries, one decimal, as a string. Decimals
    /// cross this boundary as text by house rule: a JSON number would
    /// round-trip through a float and stop being the number that was written.
    pub delay_stdev_days: String,
    /// How much of the answer is this debtor's own history — `own_history`,
    /// `blended`, or `no_history`. The field exists so a thin estimate cannot
    /// be read as a confident one.
    pub basis: String,
    pub procedure_version: String,
}

/// Why an estimate could not be produced. Deliberately narrow: every variant
/// is something the caller can fix. An unknown partner is NOT here — it is a
/// legitimate answer with `basis: no_history`, because refusing every
/// receivable of a client acquired after the fit would take the whole
/// forecast down for the one row that is least surprising.
#[derive(Debug, Clone, PartialEq)]
pub enum DelayError {
    /// `due_date` was not an ISO `YYYY-MM-DD` date.
    BadDate(String),
    /// `partner_id` was empty — a missing partner is not the same as an
    /// unknown one, and answering the pooled mean for it would attach a
    /// number to a row that names nobody.
    MissingPartner,
}

impl std::fmt::Display for DelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadDate(s) => write!(f, "due_date is not an ISO date (YYYY-MM-DD): {s}"),
            Self::MissingPartner => write!(f, "partner_id is empty — nothing to estimate for"),
        }
    }
}

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

/// Parse an ISO `YYYY-MM-DD` date. Strict on shape — an estimate keyed off a
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

/// One decimal, rounded half away from zero.
pub fn one_dp(v: f64) -> String {
    let tenths = (v * 10.0).round() as i64;
    let sign = if tenths < 0 { "-" } else { "" };
    let abs = tenths.abs();
    format!("{sign}{}.{}", abs / 10, abs % 10)
}

/// The fit for one partner, or `None` when the training window never saw it.
pub fn debtor_fit(partner_id: &str) -> Option<&'static DebtorFit> {
    DEBTORS.iter().find(|d| d.partner_id == partner_id)
}

/// How much a debtor's own mean is believed: `n / (n + K)`.
pub fn shrinkage_weight(n: u32) -> f64 {
    let n = f64::from(n);
    n / (n + SHRINKAGE_K)
}

/// The shrunk delay for one fitted debtor — the number the Procedure answers
/// with, derived rather than stored. Returned in days, unrounded.
pub fn shrunk_delay_days(fit: &DebtorFit) -> f64 {
    let w = shrinkage_weight(fit.n);
    w * fit.own_mean_days + (1.0 - w) * POOLED_MEAN_DELAY_DAYS
}

/// The estimate itself.
pub fn estimate(partner_id: &str, due_date: &str) -> Result<DelayEstimate, DelayError> {
    if partner_id.trim().is_empty() {
        return Err(DelayError::MissingPartner);
    }
    let (y, m, d) =
        parse_iso_date(due_date).ok_or_else(|| DelayError::BadDate(due_date.to_string()))?;

    let (delay, spread, basis) = match debtor_fit(partner_id) {
        Some(fit) => {
            let basis = if shrinkage_weight(fit.n) >= OWN_HISTORY_MIN_WEIGHT {
                BASIS_OWN_HISTORY
            } else {
                BASIS_BLENDED
            };
            (shrunk_delay_days(fit), fit.observed_stdev_days, basis)
        }
        // A partner the fit never saw. The pooled mean is the honest answer
        // and the basis says so — the alternative, quietly returning the same
        // number under `own_history`, is the kind of confident wrongness D7
        // exists to prevent.
        None => (
            POOLED_MEAN_DELAY_DAYS,
            POOLED_STDEV_DAYS,
            BASIS_NO_HISTORY,
        ),
    };

    let whole_days = delay.round() as i64;
    let (py, pm, pd) = civil_from_days(days_from_civil(y, m, d) + whole_days);
    Ok(DelayEstimate {
        expected_payment_date: format_iso_date(py, pm, pd),
        expected_delay_days: whole_days,
        delay_stdev_days: one_dp(spread),
        basis: basis.to_string(),
        procedure_version: MODEL_VERSION.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped training extract, read at test time so the parameters
    /// above are checkable rather than asserted.
    const EXTRACT: &str = include_str!("../training/settlements-2023-2026.csv");

    /// The audit card, read the same way. The card is the half an auditor is
    /// actually handed; nothing else in this repo compares it to the code.
    const CARD: &str = include_str!("../procedure.yaml");

    /// Rows the manifest's `trained_on` window covers — the first 174 weekly
    /// cohorts of 6, i.e. everything due before 2026-05-04.
    const TRAIN_ROWS: usize = 174 * 6;

    fn rows() -> Vec<(String, String, i64)> {
        EXTRACT
            .lines()
            .skip(1)
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                let mut it = l.split(',');
                let pid = it.next().unwrap().to_string();
                let due = it.next().unwrap().to_string();
                let _paid = it.next().unwrap();
                let delay: i64 = it.next().unwrap().parse().unwrap();
                (pid, due, delay)
            })
            .collect()
    }

    fn mean(xs: &[f64]) -> f64 {
        xs.iter().sum::<f64>() / xs.len() as f64
    }

    #[test]
    fn the_extract_is_the_window_the_card_names() {
        let r = rows();
        assert_eq!(r.len(), 1116, "186 weekly cohorts of 6 settled receivables");
        assert_eq!(r[0].1, "2023-01-02", "the epoch Monday leads the extract");
        assert_eq!(
            r[TRAIN_ROWS - 1].1,
            "2026-04-27",
            "the training window ends where `trained_on.as_of` says it does"
        );
        assert_eq!(r[r.len() - 1].1, "2026-07-20");
    }

    /// D7.2 — an auditor must be able to re-check the calculation. This IS
    /// that re-check: every baked constant re-derived from the extract.
    #[test]
    fn the_baked_parameters_are_the_fit_of_the_shipped_extract() {
        let r = rows();
        let train = &r[..TRAIN_ROWS];

        let all: Vec<f64> = train.iter().map(|(_, _, d)| *d as f64).collect();
        let pooled = mean(&all);
        assert!(
            (pooled - POOLED_MEAN_DELAY_DAYS).abs() < 1e-4,
            "POOLED_MEAN_DELAY_DAYS drifted: fitted {pooled:.4}, baked {POOLED_MEAN_DELAY_DAYS:.4}"
        );

        for fit in &DEBTORS {
            let obs: Vec<f64> = train
                .iter()
                .filter(|(p, _, _)| p == fit.partner_id)
                .map(|(_, _, d)| *d as f64)
                .collect();
            assert_eq!(
                obs.len() as u32,
                fit.n,
                "{} contributed a different number of rows than the card claims",
                fit.partner_id
            );
            let m = mean(&obs);
            assert!(
                (m - fit.own_mean_days).abs() < 1e-4,
                "{}: own_mean drifted — fitted {m:.4}, baked {:.4}",
                fit.partner_id,
                fit.own_mean_days
            );
            let var = obs.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (obs.len() - 1) as f64;
            let sd = var.sqrt();
            assert!(
                (sd - fit.observed_stdev_days).abs() < 1e-4,
                "{}: stdev drifted — fitted {sd:.4}, baked {:.4}",
                fit.partner_id,
                fit.observed_stdev_days
            );
        }

        let var = all.iter().map(|x| (x - pooled).powi(2)).sum::<f64>() / (all.len() - 1) as f64;
        assert!(
            (var.sqrt() - POOLED_STDEV_DAYS).abs() < 1e-4,
            "POOLED_STDEV_DAYS drifted: fitted {:.4}, baked {POOLED_STDEV_DAYS:.4}",
            var.sqrt()
        );
    }

    /// The holdout score on the audit card is a measurement, not a claim —
    /// and so is the baseline it is measured against. A model that beat
    /// nothing would still produce a plausible MAE.
    #[test]
    fn the_manifest_holdout_score_is_the_measured_one() {
        let r = rows();
        let holdout = &r[TRAIN_ROWS..];
        assert_eq!(holdout.len(), 72, "the holdout is the final 12 weeks");

        let errs: Vec<f64> = holdout
            .iter()
            .map(|(pid, _, actual)| {
                let pred = debtor_fit(pid)
                    .map(shrunk_delay_days)
                    .unwrap_or(POOLED_MEAN_DELAY_DAYS);
                (pred - *actual as f64).abs()
            })
            .collect();
        let mae = mean(&errs);

        // `procedure.yaml` claims holdout.score = 1.9014 (metric: mae_days).
        assert!(
            (mae - 1.9014).abs() < 5e-4,
            "the manifest claims mae_days 1.9014 but the extract measures {mae:.4}"
        );

        let base: Vec<f64> = holdout
            .iter()
            .map(|(_, _, actual)| (POOLED_MEAN_DELAY_DAYS - *actual as f64).abs())
            .collect();
        let mae_pooled = mean(&base);
        assert!(
            (mae_pooled - 4.2987).abs() < 5e-4,
            "the pooled baseline measures {mae_pooled:.4}, not the claimed 4.2987"
        );
        assert!(
            mae < mae_pooled / 2.0,
            "the whole claim of this Procedure is that per-debtor beats pooled: \
             {mae:.4} against {mae_pooled:.4}"
        );
    }

    /// The card is the half an auditor is handed, and it restates every
    /// fitted number by hand. Nothing else holds the two together.
    #[test]
    fn the_audit_card_states_what_the_component_computes() {
        assert!(
            CARD.contains(&format!("version: {MODEL_VERSION}")),
            "the card must name the version the component stamps"
        );
        assert!(CARD.contains("snapshot: settlements-2023-2026"));
        assert!(CARD.contains(&format!("pooled_mean_delay_days: {POOLED_MEAN_DELAY_DAYS}")));
        assert!(CARD.contains(&format!("pooled_stdev_days: {POOLED_STDEV_DAYS}")));
        assert!(CARD.contains(&format!("shrinkage_k: {}", SHRINKAGE_K as i64)));

        for fit in &DEBTORS {
            // Per-debtor BLOCK, not a whole-file `contains`: six of the seven
            // debtors share an `n`, so a file-wide search would pass while
            // every number sat under the wrong partner.
            let block = card_block(fit.partner_id);
            for (key, want) in [
                ("n", format!("{}", fit.n)),
                ("own_mean_days", format!("{:.4}", fit.own_mean_days)),
                (
                    "observed_stdev_days",
                    format!("{:.4}", fit.observed_stdev_days),
                ),
                // The DERIVED number: the card shows it because an accountant
                // reads the card, the code computes it because two stored
                // copies of one value drift. This is the join.
                (
                    "expected_delay_days",
                    format!("{:.4}", shrunk_delay_days(fit)),
                ),
            ] {
                let line = format!("{key}: {want}");
                assert!(
                    block.iter().any(|l| l.trim() == line),
                    "{}: the card does not state `{line}` — its block reads {block:?}",
                    fit.partner_id
                );
            }
        }
    }

    /// The card lines under one debtor's key, up to the next debtor or the
    /// end of the `debtors:` map.
    fn card_block(partner_id: &str) -> Vec<&'static str> {
        let head = format!("{partner_id}:");
        let mut out = Vec::new();
        let mut inside = false;
        for line in CARD.lines() {
            if line.trim() == head {
                inside = true;
                continue;
            }
            if inside {
                // A debtor's members are indented deeper than its own key.
                if line.trim().is_empty() || !line.starts_with("      ") {
                    break;
                }
                out.push(line);
            }
        }
        assert!(!out.is_empty(), "{partner_id} is missing from the card");
        out
    }

    #[test]
    fn a_thin_history_is_blended_and_says_so() {
        let e = estimate("BP-007", "2026-08-03").unwrap();
        assert_eq!(e.basis, BASIS_BLENDED);
        // n = 15 → weight 0.6: the answer sits between BP-007's own mean
        // (0.40) and the pooled 3.31, nearer its own.
        assert_eq!(e.expected_delay_days, 2);
        assert_eq!(e.expected_payment_date, "2026-08-05");

        let established = estimate("BP-004", "2026-08-03").unwrap();
        assert_eq!(established.basis, BASIS_OWN_HISTORY);
        assert_eq!(established.expected_delay_days, 11);
    }

    /// The failure this is here for: a creditor id, or a client acquired
    /// after the fit, must not collect a confident per-debtor answer.
    #[test]
    fn an_unseen_partner_gets_the_pooled_mean_and_the_row_says_so() {
        let e = estimate("SUP-002", "2026-08-03").unwrap();
        assert_eq!(e.basis, BASIS_NO_HISTORY);
        assert_eq!(e.expected_delay_days, 3, "the pooled mean, rounded");
        assert_eq!(
            e.delay_stdev_days, "5.7",
            "and the POOLED spread, which is much wider than any debtor's — \
             an unseen partner must not look as predictable as a known one"
        );
    }

    /// Early payers move the date BACKWARD. The sign is the whole difference
    /// between a receivable that funds Tuesday's payroll and one that does not.
    #[test]
    fn an_early_payer_settles_before_the_due_date() {
        let e = estimate("BP-006", "2026-08-03").unwrap();
        assert_eq!(e.expected_delay_days, -3);
        assert_eq!(e.expected_payment_date, "2026-07-31");
    }

    #[test]
    fn a_bad_date_or_a_missing_partner_is_refused() {
        assert!(matches!(
            estimate("BP-001", "03.08.2026"),
            Err(DelayError::BadDate(_))
        ));
        assert!(matches!(
            estimate("BP-001", "2026-02-30"),
            Err(DelayError::BadDate(_))
        ));
        assert!(matches!(
            estimate("  ", "2026-08-03"),
            Err(DelayError::MissingPartner)
        ));
    }
}
