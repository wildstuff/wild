//! `payment-delay` — ADR-0202 D5, the second dogfood Procedure.
//!
//! When will this receivable actually settle? The `cash_forecast` PROJECTION
//! in the liquidity tribe answers by assuming every debtor pays exactly on
//! the due date — which its own description admits is "exactly what the DSO
//! and the dunning lane exist to correct". This is the learned correction:
//! one mean per debtor, shrunk toward the house average when the history is
//! thin.
//!
//! Exposed twice over the same pure model, for the same reason the sibling
//! `cash-forecast` is:
//!
//!   - **`wild:function/backing`** as `settlement_delay_forecast`, so a
//!     declared field can name it with `backed_by:` (ADR-0082) and the
//!     boundary write path fills the estimate like any other enriched field
//!     (ADR-0202 D3);
//!   - **`wild:tool-provider/tools`** as `payment-delay`, so an operator (or
//!     Elder) can ask it directly without a record to hang it on.
//!
//! Both doors call [`procedure::estimate`]. An operator who sanity-checks a date
//! by hand must get the same answer the stored estimate carries, or the audit
//! card is describing a different calculation than the one that ran.
//!
//! **The answer carries its own honesty.** Every estimate states the spread
//! it was drawn from and whether it rests on the debtor's own history
//! (`own_history`), on a blend (`blended`), or on nothing but the pooled mean
//! (`no_history`). A per-debtor model that reported only a mean would present
//! a client seen fifteen times exactly like one seen a hundred and seventy.

wit_bindgen::generate!({
    path:  "wit",
    world: "payment-delay",
    generate_all,
});

pub mod procedure;

use exports::wild::function::backing::{BackingSpec, FunctionError, Guest as FunctionGuest};
use exports::wild::plugin_meta::meta::{Guest as MetaGuest, InitError, PluginKind, PluginManifest};
use exports::wild::tool_provider::tools::{
    Guest as ToolsGuest, SkillMd, ToolError, ToolResult, ToolSpec,
};

use procedure::DelayError;

struct PaymentDelay;

/// The Function backing name — the token a spec writes as
/// `backed_by: settlement_delay_forecast`.
const BACKING: &str = "settlement_delay_forecast";
/// The tool name. Lower-kebab by tool-provider convention.
const TOOL: &str = "payment-delay";

const SKILL_MD: &str = include_str!("../skills/payment-delay.md");

/// The argument bundle both doors accept. Both values are RECORD FIELDS, not
/// a call signature — a Function's inputs bind to fields of the record being
/// written, and there is no literal channel.
#[derive(serde::Deserialize)]
struct Args {
    /// Which debtor. Unknown ids are answered, not refused — see
    /// [`procedure::DelayError`].
    partner_id: String,
    /// The invoice's contractual due date — the clock the delay is measured
    /// against.
    due_date: String,
}

/// The JSON both doors answer with.
#[derive(serde::Serialize)]
struct Output {
    expected_payment_date: String,
    expected_delay_days: i64,
    delay_stdev_days: String,
    basis: String,
    procedure_version: String,
}

const INPUT_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "partner_id": {
      "type": "string",
      "description": "The debtor this invoice is owed by — normally a record field."
    },
    "due_date": {
      "type": "string",
      "description": "ISO date (YYYY-MM-DD) the invoice is contractually due — the clock the delay is measured against."
    }
  },
  "required": ["partner_id", "due_date"]
}"#;

const OUTPUT_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "expected_payment_date": {
      "type": "string",
      "description": "The date the invoice is expected to actually settle — the valid-time anchor, never the run date."
    },
    "expected_delay_days": {
      "type": "integer",
      "description": "Whole days after the due date; negative means expected early."
    },
    "delay_stdev_days": {
      "type": "string",
      "description": "The spread this debtor's payments show, in days, as a string. Wide = the date is a guess even when the mean is well fitted."
    },
    "basis": {
      "type": "string",
      "description": "own_history = this debtor's own record carries the estimate; blended = too little history, pulled toward the pooled mean; no_history = the training window never saw this partner and the answer IS the pooled mean."
    },
    "procedure_version": { "type": "string", "description": "The manifest version that produced this estimate." }
  },
  "required": ["expected_payment_date", "expected_delay_days", "delay_stdev_days", "basis", "procedure_version"]
}"#;

const DESCRIPTION: &str = "Estimate when a receivable will actually be paid, from a model fitted \
     to three and a half years of settled invoices — one mean delay per debtor, shrunk toward the \
     house average where the history is thin. Use it to answer \"when will this invoice really \
     come in\" instead of assuming the due date. It is an ESTIMATE from Procedure payment_delay \
     v1 (holdout MAE 1.9014 days, against 4.2987 for a single pooled mean), not a promise — the \
     answer carries the spread it was drawn from and says whether it rests on this debtor's own \
     history.";

/// The one path both doors take.
fn run(args_json: &str) -> Result<String, RunError> {
    let args: Args = serde_json::from_str(args_json).map_err(|e| RunError::Args(e.to_string()))?;
    let e = procedure::estimate(&args.partner_id, &args.due_date).map_err(RunError::Estimate)?;
    let out = Output {
        expected_payment_date: e.expected_payment_date,
        expected_delay_days: e.expected_delay_days,
        delay_stdev_days: e.delay_stdev_days,
        basis: e.basis,
        procedure_version: e.procedure_version,
    };
    serde_json::to_string(&out).map_err(|e| RunError::Args(e.to_string()))
}

enum RunError {
    /// The bundle did not parse against the declared input schema.
    Args(String),
    /// It parsed, but the values are outside what the model will answer for.
    Estimate(DelayError),
}

impl RunError {
    fn message(&self) -> String {
        match self {
            Self::Args(m) => m.clone(),
            Self::Estimate(e) => e.to_string(),
        }
    }
}

// ── tool-provider primitive ──

impl ToolsGuest for PaymentDelay {
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
        // Both failure modes are the CALLER's to fix, so they stay with the
        // caller as `invalid-args` rather than entering the host fault net.
        let json_output = run(&args_json).map_err(|e| ToolError::InvalidArgs(e.message()))?;
        Ok(ToolResult {
            json_output,
            // A pure-eval model costs nothing to run.
            cost_units: None,
        })
    }
}

// ── function-backing primitive (the ADR-0082 `backed_by` seam) ──

impl FunctionGuest for PaymentDelay {
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
        run(&args_json).map_err(|e| match e {
            RunError::Args(m) => FunctionError::InvalidArgs(m),
            RunError::Estimate(d) => FunctionError::InvalidArgs(d.to_string()),
        })
    }
}

// ── plugin-meta lifecycle ──

impl MetaGuest for PaymentDelay {
    fn manifest() -> PluginManifest {
        PluginManifest {
            slug: "payment-delay".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            kind: Some(PluginKind::Provider),
            provides: vec![
                "wild:tool-provider/tools@0.4.0".into(),
                "wild:function/backing@0.1.0".into(),
            ],
            // The point of a fitted model: it needs nothing from the host.
            requires: vec![],
            config_keys: vec![],
            secret_aliases: vec![],
            signatures: vec![],
        }
    }

    fn init(_config: Vec<u8>) -> Result<(), InitError> {
        // Nothing to configure — the parameters are in the binary.
        Ok(())
    }

    fn shutdown() {}
}

export!(PaymentDelay);

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_invoke(name: &str, args: &str) -> Result<ToolResult, ToolError> {
        use std::future::Future;
        use std::pin::pin;
        use std::task::{Context, Poll, Waker};
        let mut fut = pin!(<PaymentDelay as ToolsGuest>::invoke(
            name.to_string(),
            args.to_string()
        ));
        match fut.as_mut().poll(&mut Context::from_waker(&Waker::noop())) {
            Poll::Ready(v) => v,
            Poll::Pending => panic!("a pure-eval invoke must resolve in one poll"),
        }
    }

    #[test]
    fn the_manifest_declares_a_model_that_needs_nothing_from_the_host() {
        let m = PaymentDelay::manifest();
        assert_eq!(m.slug, "payment-delay");
        assert!(matches!(m.kind, Some(PluginKind::Provider)));
        assert!(
            m.requires.is_empty(),
            "a fitted model carries its parameters — importing anything here would \
             put it outside the ADR-0202 D10.3 determinism profile: {:?}",
            m.requires
        );
        assert!(m
            .provides
            .iter()
            .any(|p| p.starts_with("wild:function/backing")));
        assert!(m
            .provides
            .iter()
            .any(|p| p.starts_with("wild:tool-provider/")));
    }

    /// The name a spec writes as `backed_by:` has to be the name the
    /// component answers to, or the Function binds to nothing.
    #[test]
    fn the_backing_is_named_the_way_a_spec_binds_it() {
        let b = PaymentDelay::list_backings();
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].name, "settlement_delay_forecast");
    }

    /// The two doors are two doors onto ONE model — an operator checking by
    /// hand must not get a different date than the stored estimate.
    #[test]
    fn both_doors_answer_with_the_same_date() {
        let args = r#"{"partner_id":"BP-004","due_date":"2026-08-03"}"#;
        let via_tool = tool_invoke(TOOL, args).expect("tool").json_output;
        let via_backing =
            <PaymentDelay as FunctionGuest>::invoke(BACKING.into(), args.into()).expect("backing");
        assert_eq!(via_tool, via_backing);
    }

    #[test]
    fn the_answer_carries_its_model_version_and_its_basis() {
        let out = <PaymentDelay as FunctionGuest>::invoke(
            BACKING.into(),
            r#"{"partner_id":"BP-004","due_date":"2026-08-03"}"#.into(),
        )
        .expect("backing");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["procedure_version"], "v1");
        assert_eq!(v["expected_payment_date"], "2026-08-14");
        assert_eq!(v["basis"], "own_history");
        assert!(
            v["delay_stdev_days"].is_string(),
            "decimals cross this boundary as text"
        );
    }

    /// The shape the liquidity tribe depends on: one call per open
    /// receivable, and the DEBTOR is what moves the date. Nothing in the
    /// tribe's YAML would fail to compile if this stopped honouring
    /// `partner_id` — every invoice would simply land on the same offset,
    /// which is the pooled model the Procedure exists to replace.
    #[test]
    fn two_debtors_with_one_due_date_settle_on_different_days() {
        let dates: Vec<String> = ["BP-006", "BP-002", "BP-003", "BP-001", "BP-005", "BP-004"]
            .iter()
            .map(|pid| {
                let out = <PaymentDelay as FunctionGuest>::invoke(
                    BACKING.into(),
                    format!(r#"{{"partner_id":"{pid}","due_date":"2026-08-03"}}"#),
                )
                .expect("backing");
                let v: serde_json::Value = serde_json::from_str(&out).unwrap();
                v["expected_payment_date"].as_str().unwrap().to_string()
            })
            .collect();

        let unique: std::collections::BTreeSet<&String> = dates.iter().collect();
        assert_eq!(
            unique.len(),
            dates.len(),
            "six debtors with distinct fitted means must land on six distinct \
             dates, or the per-debtor split is not reaching the answer: {dates:?}"
        );
        // Listed in fitted order, so the dates must come out ascending.
        let mut sorted = dates.clone();
        sorted.sort();
        assert_eq!(dates, sorted, "earliest payer first: {dates:?}");
    }

    #[test]
    fn a_bad_bundle_stays_with_the_caller() {
        assert!(matches!(
            <PaymentDelay as FunctionGuest>::invoke(BACKING.into(), "not json".into()),
            Err(FunctionError::InvalidArgs(_))
        ));
        // A missing REQUIRED field is a caller error too — the model must not
        // be handed a default partner.
        assert!(matches!(
            <PaymentDelay as FunctionGuest>::invoke(
                BACKING.into(),
                r#"{"due_date":"2026-08-03"}"#.into()
            ),
            Err(FunctionError::InvalidArgs(_))
        ));
        assert!(matches!(
            tool_invoke(TOOL, r#"{"partner_id":"BP-001","due_date":"03.08.2026"}"#),
            Err(ToolError::InvalidArgs(_))
        ));
    }

    #[test]
    fn an_unknown_name_is_refused_at_both_doors() {
        assert!(matches!(
            <PaymentDelay as FunctionGuest>::invoke("nope".into(), "{}".into()),
            Err(FunctionError::UnknownBacking(_))
        ));
        assert!(matches!(
            tool_invoke("nope", "{}"),
            Err(ToolError::UnknownTool(_))
        ));
    }

    /// The description is what an operator and the model both read before
    /// choosing this tool. A score with no baseline beside it is a number
    /// nobody can judge, so both belong in the sentence.
    #[test]
    fn the_description_names_the_score_and_what_it_beats() {
        // The CARD's figures, not rounded ones: the tribe's Function
        // description and the skill file quote the same two numbers, and a
        // rounded copy here would be a fourth spelling of one measurement.
        assert!(
            DESCRIPTION.contains("1.9014"),
            "the holdout MAE, as the card states it"
        );
        assert!(
            DESCRIPTION.contains("4.2987"),
            "the pooled baseline it beats, as the card states it"
        );
        assert!(DESCRIPTION.contains("payment_delay v1"));
    }

    #[test]
    fn the_tool_ships_its_skill_md() {
        let s = PaymentDelay::list_skill_mds();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].slug, TOOL);
        assert!(s[0].body.contains("settlement_delay_forecast"));
    }
}
