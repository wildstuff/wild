//! `cash-forecast` — ADR-0202 D5, the dogfood Procedure.
//!
//! A seasonal cash-inflow forecast, exposed twice over the same pure model:
//!
//!   - **`wild:function/backing`** as `cash_forecast`, so a declared field
//!     can name it with `backed_by: cash_forecast` (ADR-0082) and the
//!     boundary write path fills the estimate like any other enriched
//!     field (ADR-0202 D3);
//!   - **`wild:tool-provider/tools`** as `cash-forecast`, so an operator
//!     (or Elder) can ask it directly without a record to hang it on.
//!
//! Both doors call [`procedure::forecast`]. That is deliberate: an operator who
//! sanity-checks the number by hand must get the same answer the stored
//! estimate carries, or the audit card is describing a different
//! calculation than the one that ran.
//!
//! **Arguments are record fields, not a call signature.** ADR-0202's
//! sketch reads `forecast_cash_position(days: 30)`, but a Function's inputs
//! bind to fields of the record being written — there is no literal
//! channel. So `as_of` arrives from a field and the horizon arrives from
//! the Function's declared `config`; both reach this component as one
//! `args-json` bundle.

wit_bindgen::generate!({
    path:  "wit",
    world: "cash-forecast",
    generate_all,
});

pub mod procedure;

use exports::wild::function::backing::{BackingSpec, FunctionError, Guest as FunctionGuest};
use exports::wild::plugin_meta::meta::{Guest as MetaGuest, InitError, PluginKind, PluginManifest};
use exports::wild::tool_provider::tools::{
    Guest as ToolsGuest, SkillMd, ToolError, ToolResult, ToolSpec,
};

use procedure::ForecastError;

struct CashForecast;

/// The Function backing name — the token a spec writes as
/// `backed_by: cash_forecast`.
const BACKING: &str = "cash_forecast";
/// The tool name. Lower-kebab by tool-provider convention.
const TOOL: &str = "cash-forecast";

const SKILL_MD: &str = include_str!("../skills/cash-forecast.md");

/// The argument bundle both doors accept.
#[derive(serde::Deserialize)]
struct Args {
    /// The date the forecast is made FROM — normally a record field.
    as_of: String,
    /// How far ahead to forecast. Arrives from the Function's declared
    /// `config`; defaults to the 30 days ADR-0202 D5 names.
    #[serde(default = "default_horizon")]
    horizon_days: i64,
}

fn default_horizon() -> i64 {
    30
}

/// The JSON both doors answer with.
#[derive(serde::Serialize)]
struct Output {
    target_date: String,
    expected_inflow: String,
    week_bucket: u32,
    procedure_version: String,
}

const INPUT_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "as_of": {
      "type": "string",
      "description": "ISO date (YYYY-MM-DD) the forecast is made from — normally a record field."
    },
    "horizon_days": {
      "type": "integer",
      "minimum": 0,
      "maximum": 365,
      "default": 30,
      "description": "Days ahead to forecast. Beyond 365 the seasonal fit says nothing useful and the call is refused."
    }
  },
  "required": ["as_of"]
}"#;

const OUTPUT_SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "target_date": {
      "type": "string",
      "description": "The date the estimate is ABOUT — the valid-time anchor, never the run date."
    },
    "expected_inflow": {
      "type": "string",
      "description": "Expected inflow in euro, two decimals, as a string (a JSON number would round-trip through a float)."
    },
    "week_bucket": { "type": "integer", "description": "Which weekly bucket answered (1..52)." },
    "procedure_version": { "type": "string", "description": "The manifest version that produced this estimate." }
  },
  "required": ["target_date", "expected_inflow", "week_bucket", "procedure_version"]
}"#;

const DESCRIPTION: &str = "Forecast expected cash inflow for a date some days ahead, from a \
     seasonal model fitted to two years of weekly payment inflows. Use it to answer \"what do we \
     expect to come in around <date>\". It is an ESTIMATE from Procedure cash_forecast v3 \
     (holdout MAPE 0.038), not a booked figure — the answer carries its model version so a \
     later refit does not silently rewrite what was already reported.";

/// The one path both doors take.
fn run(args_json: &str) -> Result<String, RunError> {
    let args: Args = serde_json::from_str(args_json).map_err(|e| RunError::Args(e.to_string()))?;
    let f = procedure::forecast(&args.as_of, args.horizon_days).map_err(RunError::Forecast)?;
    let out = Output {
        target_date: f.target_date,
        expected_inflow: f.expected_inflow,
        week_bucket: f.week_bucket,
        procedure_version: f.procedure_version,
    };
    serde_json::to_string(&out).map_err(|e| RunError::Args(e.to_string()))
}

enum RunError {
    /// The bundle did not parse against the declared input schema.
    Args(String),
    /// It parsed, but the values are outside what the model will answer for.
    Forecast(ForecastError),
}

impl RunError {
    fn message(&self) -> String {
        match self {
            Self::Args(m) => m.clone(),
            Self::Forecast(e) => e.to_string(),
        }
    }
}

// ── tool-provider primitive ──

impl ToolsGuest for CashForecast {
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

impl FunctionGuest for CashForecast {
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
            RunError::Forecast(f) => FunctionError::InvalidArgs(f.to_string()),
        })
    }
}

// ── plugin-meta lifecycle ──

impl MetaGuest for CashForecast {
    fn manifest() -> PluginManifest {
        PluginManifest {
            slug: "cash-forecast".into(),
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

export!(CashForecast);

#[cfg(test)]
mod tests {
    use super::*;
    use procedure::MAX_HORIZON_DAYS;

    fn tool_invoke(name: &str, args: &str) -> Result<ToolResult, ToolError> {
        use std::future::Future;
        use std::pin::pin;
        use std::task::{Context, Poll, Waker};
        let mut fut = pin!(<CashForecast as ToolsGuest>::invoke(
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
        let m = CashForecast::manifest();
        assert_eq!(m.slug, "cash-forecast");
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
        let b = CashForecast::list_backings();
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].name, "cash_forecast");
    }

    /// The two doors are two doors onto ONE model — an operator checking by
    /// hand must not get a different number than the stored estimate.
    #[test]
    fn both_doors_answer_with_the_same_number() {
        let args = r#"{"as_of":"2025-01-06","horizon_days":30}"#;
        let via_tool = tool_invoke(TOOL, args).expect("tool").json_output;
        let via_backing =
            <CashForecast as FunctionGuest>::invoke(BACKING.into(), args.into()).expect("backing");
        assert_eq!(via_tool, via_backing);
    }

    #[test]
    fn the_answer_carries_its_model_version_and_target_date() {
        let out = <CashForecast as FunctionGuest>::invoke(
            BACKING.into(),
            r#"{"as_of":"2025-01-06","horizon_days":30}"#.into(),
        )
        .expect("backing");
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["procedure_version"], "v4");
        assert_eq!(v["target_date"], "2025-02-05");
        assert!(v["expected_inflow"].is_string(), "money crosses as text");
    }

    /// The shape the liquidity tribe now depends on: the horizon arrives as a
    /// RECORD FIELD, one run answers several of them, and each answer is about
    /// its own week.
    ///
    /// It has its own test because the dependency is invisible from here. The
    /// tribe declares `inputs: [as_of, horizon_days]`, and the enrich worker
    /// builds the args bundle by copying those FIELDS off the record — so the
    /// horizon is no longer a constant this component chose but a value the
    /// caller supplies per row. Nothing in the tribe's YAML would fail to
    /// compile if this stopped honouring it; the curve would just answer 13
    /// times for the same week.
    #[test]
    fn one_as_of_answers_a_whole_weekly_curve() {
        let as_of = "2026-07-27"; // a Monday
        let mut targets = Vec::new();
        for week in 1..=13u32 {
            let out = <CashForecast as FunctionGuest>::invoke(
                BACKING.into(),
                format!(r#"{{"as_of":"{as_of}","horizon_days":{}}}"#, week * 7),
            )
            .expect("backing");
            let v: serde_json::Value = serde_json::from_str(&out).unwrap();
            targets.push(v["target_date"].as_str().unwrap().to_string());
        }

        assert_eq!(targets[0], "2026-08-03", "the first point is the next week");
        assert_eq!(targets[12], "2026-10-26", "the thirteenth is 91 days out");
        let unique: std::collections::BTreeSet<&String> = targets.iter().collect();
        assert_eq!(
            unique.len(),
            targets.len(),
            "every horizon must land on its own week, or the tribe's estimate \
             entity — keyed on [target_date, procedure_version] — collapses the \
             curve onto one row: {targets:?}"
        );
    }

    #[test]
    fn the_horizon_defaults_to_the_thirty_days_the_adr_names() {
        let with = <CashForecast as FunctionGuest>::invoke(
            BACKING.into(),
            r#"{"as_of":"2025-01-06","horizon_days":30}"#.into(),
        )
        .unwrap();
        let without = <CashForecast as FunctionGuest>::invoke(
            BACKING.into(),
            r#"{"as_of":"2025-01-06"}"#.into(),
        )
        .unwrap();
        assert_eq!(with, without);
    }

    #[test]
    fn a_bad_bundle_stays_with_the_caller() {
        assert!(matches!(
            <CashForecast as FunctionGuest>::invoke(BACKING.into(), "not json".into()),
            Err(FunctionError::InvalidArgs(_))
        ));
        assert!(matches!(
            <CashForecast as FunctionGuest>::invoke(
                BACKING.into(),
                r#"{"as_of":"07.01.2025"}"#.into()
            ),
            Err(FunctionError::InvalidArgs(_))
        ));
        assert!(matches!(
            tool_invoke(TOOL, r#"{"as_of":"2025-01-06","horizon_days":9999}"#),
            Err(ToolError::InvalidArgs(_))
        ));
    }

    #[test]
    fn an_unknown_name_is_refused_at_both_doors() {
        assert!(matches!(
            <CashForecast as FunctionGuest>::invoke("nope".into(), "{}".into()),
            Err(FunctionError::UnknownBacking(_))
        ));
        assert!(matches!(
            tool_invoke("nope", "{}"),
            Err(ToolError::UnknownTool(_))
        ));
    }

    #[test]
    fn the_declared_horizon_ceiling_matches_the_schema() {
        assert!(INPUT_SCHEMA.contains(&MAX_HORIZON_DAYS.to_string()));
    }

    #[test]
    fn the_tool_ships_its_skill_md() {
        let s = CashForecast::list_skill_mds();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].slug, TOOL);
        assert!(s[0].body.contains("cash_forecast"));
    }
}
