//! `fx-exposure` — ADR-0202 D5, the two-axis Procedure.
//!
//! What is a foreign-currency receivable actually worth in euro by the time it
//! settles? The euro amount is `amount / rate`, which is arithmetic. What is
//! learned is how far that rate typically moves in the meantime — one
//! volatility per currency — so the answer is a **band**, and the band is what
//! a treasurer can plan against.
//!
//! Exposed twice over the same pure model, like both siblings:
//!
//!   - **`wild:function/backing`** as `fx_exposure_valuation`, so a declared
//!     field can name it with `backed_by:` (ADR-0082) and the boundary write
//!     path fills the valuation like any other enriched field;
//!   - **`wild:tool-provider/tools`** as `fx-exposure`, so an operator (or
//!     Elder) can ask it directly without a record to hang it on.
//!
//! **The rate arrives as an argument, and that is not a design preference.**
//! ADR-0202 D10.3's import allow-list carries no `wasi:sockets`, no
//! `wasi:http` and no `wild:http`, so a Procedure cannot fetch its own market
//! data. The impure half has to be a connector. What looks like a limitation
//! is what makes a stored valuation auditable: the rate and its date are on
//! the row, so "why did this say 41.200 €" has an answer three years later.
//!
//! Which is also why `rate_date` rides back OUT with every answer even though
//! the caller supplied it. The row that stores the number has to carry the
//! snapshot it was anchored to, or the second audit axis exists only in
//! whatever called the model.

wit_bindgen::generate!({
    path:  "wit",
    world: "fx-exposure",
    generate_all,
});

pub mod procedure;

use exports::wild::function::backing::{BackingSpec, FunctionError, Guest as FunctionGuest};
use exports::wild::plugin_meta::meta::{Guest as MetaGuest, InitError, PluginKind, PluginManifest};
use exports::wild::tool_provider::tools::{
    Guest as ToolsGuest, SkillMd, ToolError, ToolResult, ToolSpec,
};

use procedure::ValuationError;

struct FxExposure;

/// The Function backing name — the token a spec writes as
/// `backed_by: fx_exposure_valuation`.
const BACKING: &str = "fx_exposure_valuation";
/// The tool name. Lower-kebab by tool-provider convention.
const TOOL: &str = "fx-exposure";

const SKILL_MD: &str = include_str!("../skills/fx-exposure.md");

/// The argument bundle both doors accept. Every value is a RECORD FIELD — a
/// Function's inputs bind to fields of the record being written, and there is
/// no literal channel.
#[derive(serde::Deserialize)]
struct Args {
    /// ISO 4217 code of the receivable.
    currency: String,
    /// The invoice amount, in that currency. Text on the wire: a JSON number
    /// would round-trip through a float and stop being the amount that was
    /// written.
    amount_foreign: String,
    /// Units of `currency` per ONE euro — the ECB quoting convention.
    rate_per_eur: String,
    /// The day that rate was quoted. Half the audit answer.
    rate_date: String,
    /// Calendar days until the money is expected to arrive. Normally the
    /// `payment-delay` Procedure's answer, which is what makes the two
    /// compose.
    days_to_settlement: i64,
}

/// The JSON both doors answer with.
#[derive(serde::Serialize)]
struct Output {
    expected_eur: String,
    band_low_eur: String,
    band_high_eur: String,
    band_pct: String,
    rate_date: String,
    basis: String,
    procedure_version: String,
}

const INPUT_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "currency": { "type": "string", "description": "ISO 4217 code of the receivable (CHF, GBP, USD; EUR is answered as itself)." },
    "amount_foreign": { "type": "string", "description": "Invoice amount in that currency, as a string." },
    "rate_per_eur": { "type": "string", "description": "Reference rate: units of the currency per ONE euro (the ECB convention)." },
    "rate_date": { "type": "string", "description": "ISO date (YYYY-MM-DD) the rate was quoted — half the audit answer, so it is required and never guessed." },
    "days_to_settlement": { "type": "integer", "minimum": 0, "description": "Calendar days until the money is expected to arrive; 0 values it today." }
  },
  "required": ["currency", "amount_foreign", "rate_per_eur", "rate_date", "days_to_settlement"]
}"#;

const OUTPUT_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "expected_eur": { "type": "string", "description": "The receivable's euro value at the anchoring rate." },
    "band_low_eur": { "type": "string", "description": "Low end of the 80% band — what it is worth if the euro strengthens." },
    "band_high_eur": { "type": "string", "description": "High end of the 80% band." },
    "band_pct": { "type": "string", "description": "Half-width as a percentage of the expected value; 0.0 for a euro receivable." },
    "rate_date": { "type": "string", "description": "The snapshot this answer is anchored to, echoed back so the stored row carries the second audit axis." },
    "basis": { "type": "string", "description": "fitted = a fitted volatility carried the band; no_fx_risk = a euro receivable, which has none." },
    "procedure_version": { "type": "string", "description": "The manifest version that produced this valuation." }
  },
  "required": ["expected_eur", "band_low_eur", "band_high_eur", "band_pct", "rate_date", "basis", "procedure_version"]
}"#;

const DESCRIPTION: &str = "Value a foreign-currency receivable in euro at a given reference rate, \
     with an 80% uncertainty band from a volatility fitted per currency over seven years of daily \
     rates. Use it to answer \"what is this dollar invoice really worth by the time it is paid\". \
     It is an ESTIMATE from Procedure fx_exposure v1 (holdout band coverage 0.7595 against a 0.80 \
     target) and it is only defined RELATIVE TO THE RATE SNAPSHOT it is given — the answer carries \
     that snapshot's date back, so a stored valuation says what it was anchored to. An unfitted \
     currency is refused rather than averaged.";

/// The one path both doors take.
fn run(args_json: &str) -> Result<String, RunError> {
    let a: Args = serde_json::from_str(args_json).map_err(|e| RunError::Args(e.to_string()))?;
    let amount: f64 = a
        .amount_foreign
        .trim()
        .parse()
        .map_err(|_| RunError::Args(format!("amount_foreign is not a number: {}", a.amount_foreign)))?;
    let rate: f64 = a
        .rate_per_eur
        .trim()
        .parse()
        .map_err(|_| RunError::Args(format!("rate_per_eur is not a number: {}", a.rate_per_eur)))?;

    let v = procedure::value(&a.currency, amount, rate, &a.rate_date, a.days_to_settlement)
        .map_err(RunError::Valuation)?;
    let out = Output {
        expected_eur: v.expected_eur,
        band_low_eur: v.band_low_eur,
        band_high_eur: v.band_high_eur,
        band_pct: v.band_pct,
        rate_date: v.rate_date,
        basis: v.basis,
        procedure_version: v.procedure_version,
    };
    serde_json::to_string(&out).map_err(|e| RunError::Args(e.to_string()))
}

enum RunError {
    /// The bundle did not parse against the declared input schema.
    Args(String),
    /// It parsed, but the values are outside what the model will answer for.
    Valuation(ValuationError),
}

impl RunError {
    fn message(&self) -> String {
        match self {
            Self::Args(m) => m.clone(),
            Self::Valuation(e) => e.to_string(),
        }
    }
}

// ── tool-provider primitive ──

impl ToolsGuest for FxExposure {
    fn list_tools() -> Vec<ToolSpec> {
        vec![ToolSpec {
            name: TOOL.into(),
            description: DESCRIPTION.into(),
            json_schema: INPUT_SCHEMA.into(),
        }]
    }

    fn list_skill_mds() -> Vec<SkillMd> {
        vec![SkillMd {
            slug: TOOL.into(),
            body: SKILL_MD.into(),
        }]
    }

    #[allow(clippy::unused_async)]
    async fn invoke(name: String, args_json: String) -> Result<ToolResult, ToolError> {
        if name != TOOL {
            return Err(ToolError::UnknownTool(name));
        }
        let json_output = run(&args_json).map_err(|e| ToolError::InvalidArgs(e.message()))?;
        Ok(ToolResult {
            json_output,
            cost_units: None,
        })
    }
}

// ── function-backing primitive (the ADR-0082 `backed_by` seam) ──

impl FunctionGuest for FxExposure {
    fn list_backings() -> Vec<BackingSpec> {
        vec![BackingSpec {
            name: BACKING.into(),
            description: DESCRIPTION.into(),
            input_schema: INPUT_SCHEMA.into(),
            output_schema: OUTPUT_SCHEMA.into(),
        }]
    }

    fn invoke(name: String, args_json: String) -> Result<String, FunctionError> {
        if name != BACKING {
            return Err(FunctionError::UnknownBacking(name));
        }
        run(&args_json).map_err(|e| FunctionError::InvalidArgs(e.message()))
    }
}

// ── plugin-meta lifecycle ──

impl MetaGuest for FxExposure {
    fn manifest() -> PluginManifest {
        PluginManifest {
            slug: "fx-exposure".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            kind: Some(PluginKind::Provider),
            provides: vec![
                "wild:tool-provider/tools@0.4.0".into(),
                "wild:function/backing@0.1.0".into(),
            ],
            // Nothing — and for this Procedure that is the whole lesson. The
            // one component in the family that has an obvious reason to reach
            // the network is the one the profile most needs to stop.
            requires: vec![],
            config_keys: vec![],
            secret_aliases: vec![],
            signatures: vec![],
        }
    }

    fn init(_config: Vec<u8>) -> Result<(), InitError> {
        Ok(())
    }

    fn shutdown() {}
}

export!(FxExposure);

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_invoke(name: &str, args: &str) -> Result<ToolResult, ToolError> {
        use std::future::Future;
        use std::pin::pin;
        use std::task::{Context, Poll, Waker};
        let mut fut = pin!(<FxExposure as ToolsGuest>::invoke(
            name.to_string(),
            args.to_string()
        ));
        match fut.as_mut().poll(&mut Context::from_waker(&Waker::noop())) {
            Poll::Ready(v) => v,
            Poll::Pending => panic!("a pure-eval invoke must resolve in one poll"),
        }
    }

    const USD_ARGS: &str = r#"{"currency":"USD","amount_foreign":"12000.00",
        "rate_per_eur":"1.0850","rate_date":"2026-07-21","days_to_settlement":30}"#;

    /// The claim the WIT world makes in prose, asserted where it can fail.
    #[test]
    fn the_verfahren_that_wants_the_network_most_still_imports_nothing() {
        let m = FxExposure::manifest();
        assert_eq!(m.slug, "fx-exposure");
        assert!(matches!(m.kind, Some(PluginKind::Provider)));
        assert!(
            m.requires.is_empty(),
            "a Procedure may not fetch its own market data — the rate has to \
             arrive as a field or the snapshot it was anchored to is nowhere \
             on the row: {:?}",
            m.requires
        );
    }

    #[test]
    fn the_backing_is_named_the_way_a_spec_binds_it() {
        let b = FxExposure::list_backings();
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].name, "fx_exposure_valuation");
    }

    #[test]
    fn both_doors_answer_with_the_same_band() {
        let via_tool = tool_invoke(TOOL, USD_ARGS).expect("tool").json_output;
        let via_backing = <FxExposure as FunctionGuest>::invoke(BACKING.into(), USD_ARGS.into())
            .expect("backing");
        assert_eq!(via_tool, via_backing);
    }

    #[test]
    fn the_answer_carries_its_version_and_its_anchor() {
        let out = <FxExposure as FunctionGuest>::invoke(BACKING.into(), USD_ARGS.into()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["procedure_version"], "v1");
        assert_eq!(v["rate_date"], "2026-07-21");
        assert_eq!(v["basis"], "fitted");
        assert_eq!(v["expected_eur"], "11059.91");
        for k in ["expected_eur", "band_low_eur", "band_high_eur", "band_pct"] {
            assert!(v[k].is_string(), "{k} crosses as text");
        }
    }

    /// Money crosses as TEXT, and the test exists because the tempting
    /// alternative silently loses cents: a JSON number round-trips through an
    /// f64 and stops being the amount that was written.
    #[test]
    fn a_numeric_amount_is_refused_rather_than_coerced() {
        let numeric = r#"{"currency":"USD","amount_foreign":12000.00,
            "rate_per_eur":"1.0850","rate_date":"2026-07-21","days_to_settlement":30}"#;
        assert!(matches!(
            <FxExposure as FunctionGuest>::invoke(BACKING.into(), numeric.into()),
            Err(FunctionError::InvalidArgs(_))
        ));
    }

    #[test]
    fn a_bad_bundle_stays_with_the_caller() {
        assert!(matches!(
            <FxExposure as FunctionGuest>::invoke(BACKING.into(), "not json".into()),
            Err(FunctionError::InvalidArgs(_))
        ));
        // A missing rate_date is a caller error, never a blank on the row.
        let no_date = r#"{"currency":"USD","amount_foreign":"1.00",
            "rate_per_eur":"1.0850","days_to_settlement":30}"#;
        assert!(matches!(
            <FxExposure as FunctionGuest>::invoke(BACKING.into(), no_date.into()),
            Err(FunctionError::InvalidArgs(_))
        ));
        assert!(matches!(
            tool_invoke(TOOL, r#"{"currency":"JPY","amount_foreign":"1.00",
                "rate_per_eur":"168.0","rate_date":"2026-07-21","days_to_settlement":30}"#),
            Err(ToolError::InvalidArgs(_))
        ));
    }

    #[test]
    fn an_unknown_name_is_refused_at_both_doors() {
        assert!(matches!(
            <FxExposure as FunctionGuest>::invoke("nope".into(), "{}".into()),
            Err(FunctionError::UnknownBacking(_))
        ));
        assert!(matches!(
            tool_invoke("nope", "{}"),
            Err(ToolError::UnknownTool(_))
        ));
    }

    /// The description is what an operator and the model both read before
    /// choosing this tool, and the anchor requirement is the one thing about
    /// this Procedure that cannot be inferred from its name.
    #[test]
    fn the_description_names_the_score_and_the_anchor_requirement() {
        assert!(DESCRIPTION.contains("0.7595"), "the measured coverage");
        assert!(DESCRIPTION.contains("0.80"), "the target it is judged against");
        assert!(
            DESCRIPTION.contains("RELATIVE TO THE RATE SNAPSHOT"),
            "an answer whose anchor is invisible reads as absolute"
        );
        assert!(DESCRIPTION.contains("fx_exposure v1"));
    }

    #[test]
    fn the_tool_ships_its_skill_md() {
        let s = FxExposure::list_skill_mds();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].slug, TOOL);
        assert!(s[0].body.contains("fx_exposure_valuation"));
    }
}

#[cfg(test)]
mod worked_example {
    use super::*;

    /// `1234567.89` → `1.234.567,89`, the way the skill file writes money.
    fn de(v: &str) -> String {
        let (int, frac) = v.split_once('.').expect("two decimals");
        let mut grouped = String::new();
        for (i, c) in int.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                grouped.push('.');
            }
            grouped.push(c);
        }
        format!("{},{frac}", grouped.chars().rev().collect::<String>())
    }

    /// The README repeats the same worked example, so it is checked with it.
    /// A number written into two files is two numbers as soon as one moves.
    const README: &str = include_str!("../README.md");

    /// The worked example in the skill file is arithmetic, so it is checked.
    ///
    /// The failure this exists for is one I wrote twice in this family before
    /// catching it: a specific, plausible, wrong number in prose. Nothing
    /// errors on it — it reads as authoritative BECAUSE it is specific, and a
    /// reader checking the Procedure against it concludes the Procedure is
    /// broken. The band ends are the easiest of all to get wrong by hand,
    /// because the euro band is not symmetric about the middle.
    #[test]
    fn the_skill_files_example_is_what_the_model_answers() {
        let v = procedure::value("USD", 12_000.0, 1.0850, "2026-07-21", 30).expect("valuation");
        for (label, amount) in [
            ("expected", &v.expected_eur),
            ("low", &v.band_low_eur),
            ("high", &v.band_high_eur),
        ] {
            let want = format!("{} €", de(amount));
            for (file, text) in [("skills/fx-exposure.md", SKILL_MD), ("README.md", README)] {
                assert!(
                    text.contains(&want),
                    "{file}'s {label} figure is not `{want}` — the model \
                     answers {} / {} / {}",
                    v.expected_eur,
                    v.band_low_eur,
                    v.band_high_eur
                );
            }
        }
        assert!(
            SKILL_MD.contains(&format!("{} %", v.band_pct.replace('.', ","))),
            "the skill file does not quote the band width the model computes ({})",
            v.band_pct
        );
    }
}
