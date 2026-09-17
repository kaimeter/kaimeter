//! Conformance test for the two-supplier precursor vector of whitepaper §11.
//!
//! The extruder's billet arrives from two smelters; the vector fixes the
//! suppliers' specific embedded emissions, their consumed quantities and the
//! extruder's own direct emissions. Both the plain evaluator and the
//! interpreted `rules.json` must reproduce the Art. 14 weighted average and
//! the complex good's specific embedded emissions at full precision.

use std::collections::BTreeMap;

use kaimeter_interpreter::rule::RuleBundle;
use kaimeter_rules::bundle::BundleFile;
use kaimeter_rules::common::precursors::{
    PrecursorOrigin, PrecursorSupply, see_complex, weighted_average,
};
use kaimeter_rules::fixed::Fixed;
use serde::Deserialize;

/// The crate's conformance copy of the §11 vector.
const VECTOR: &str = include_str!("vectors/aluminium/two-supplier-precursors.json");

/// One precursor supply of the vector.
#[derive(Deserialize)]
struct Supply {
    see_tco2e_per_t: String,
    quantity_t: String,
    origin: String,
}

/// The §11 two-supplier vector.
#[derive(Deserialize)]
struct Vector {
    bundle: String,
    sector: String,
    route: String,
    cn_code: String,
    period: String,
    method: String,
    supplies: Vec<Supply>,
    inputs: BTreeMap<String, String>,
    expected: BTreeMap<String, String>,
}

fn fixed(text: &str) -> Fixed {
    text.parse().unwrap()
}

#[test]
fn two_supplier_vector_passes_at_full_precision() {
    let vector: Vector = serde_json::from_str(VECTOR).unwrap();
    assert_eq!(vector.bundle, "cbam");
    assert_eq!(vector.sector, "aluminium");
    assert_eq!(vector.route, "complex");
    assert_eq!(vector.cn_code, "7604 10 90");
    assert_eq!(vector.method, "art14_two_supplier_precursors");

    let supplies: Vec<PrecursorSupply> = vector
        .supplies
        .iter()
        .map(|supply| PrecursorSupply {
            see: fixed(&supply.see_tco2e_per_t),
            quantity_t: fixed(&supply.quantity_t),
            origin: match supply.origin.as_str() {
                "third_country" => PrecursorOrigin::ThirdCountry,
                "eu_or_excluded" => PrecursorOrigin::EuOrExcluded,
                other => panic!("unknown origin `{other}`"),
            },
        })
        .collect();

    let average = weighted_average(&supplies).unwrap();
    assert_eq!(
        average.to_string(),
        vector.expected["weighted_average_tco2e_per_t"]
    );

    let direct_tco2e_t = fixed(&vector.inputs["direct_tco2e_t"]);
    let production_t = fixed(&vector.inputs["production_t"]);
    let see = see_complex(direct_tco2e_t, production_t, &supplies).unwrap();
    assert_eq!(see.to_string(), vector.expected["see_tco2e_per_t"]);
}

#[test]
fn interpreted_rules_agree_with_the_two_supplier_vector() {
    let vector: Vector = serde_json::from_str(VECTOR).unwrap();

    let files = [BundleFile {
        path: "rules.json",
        content: include_bytes!("../rules.json"),
    }];
    let bundle = RuleBundle::from_files(&files).unwrap();

    let supplies: Vec<_> = vector
        .supplies
        .iter()
        .map(|supply| {
            serde_json::json!({
                "see_tco2e_per_t": supply.see_tco2e_per_t,
                "quantity_t": supply.quantity_t,
                "origin": supply.origin,
            })
        })
        .collect();
    let invocation = serde_json::json!({
        "rule": "common.complex.see",
        "context": {
            "sector": vector.sector,
            "route": vector.route,
            "cnCode": vector.cn_code,
            "period": vector.period,
        },
        "inputs": {
            "production_t": vector.inputs["production_t"],
            "direct_tco2e_t": vector.inputs["direct_tco2e_t"],
            "supplies": supplies,
        },
    });
    let invocation = bundle.parse_invocation(&invocation.to_string()).unwrap();
    let outcome = bundle.evaluate(&invocation).unwrap();

    assert_eq!(outcome.proves, "see_tco2e_per_t");
    assert_eq!(
        outcome.output.to_string(),
        vector.expected["see_tco2e_per_t"]
    );
}
