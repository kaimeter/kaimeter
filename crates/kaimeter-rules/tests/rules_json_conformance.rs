//! Conformance of the interpreted `rules.json` with the Appendix B vector.
//!
//! The rules travel as data (interpreter contract §3): the committed
//! `rules.json` and parameter tables, evaluated through the fixed interpreter,
//! must reproduce every expected value of the worked example at the bundle's
//! full fixed-point precision.

use std::collections::BTreeMap;

use kaimeter_interpreter::rule::RuleBundle;
use kaimeter_rules::bundle::BundleFile;
use serde::Deserialize;

/// The crate's copy of the published Appendix B vector.
const VECTOR: &str = include_str!("vectors/aluminium/worked-example.json");

/// Rule outputs, paired with the vector key each must equal.
const EXPECTED: [(&str, &str); 4] = [
    ("cf4_t", "e_cf4_t"),
    ("c2f6_t", "e_c2f6_t"),
    ("pfc_tco2e", "e_pfc_tco2e"),
    ("see_tco2e_per_t", "see_tco2e_per_t"),
];

/// The parts of the Appendix B vector this test consumes.
#[derive(Deserialize)]
struct Vector {
    sector: String,
    route: String,
    cn_code: String,
    period: String,
    inputs: BTreeMap<String, String>,
    expected: BTreeMap<String, String>,
}

#[test]
fn rules_json_aggregates_a_two_supplier_complex_good() {
    let files = [BundleFile {
        path: "rules.json",
        content: include_bytes!("../rules.json"),
    }];
    let bundle = RuleBundle::from_files(&files).unwrap();

    let invocation = serde_json::json!({
        "rule": "common.complex.see",
        "context": {
            "sector": "aluminium",
            "route": "extrusion",
            "cnCode": "7604 10 90",
            "period": "2026",
        },
        "inputs": {
            "production_t": "1000",
            "direct_tco2e_t": "120",
            "supplies": [
                {"see_tco2e_per_t": "1.835038", "quantity_t": "600", "origin": "third_country"},
                {"see_tco2e_per_t": "1.910", "quantity_t": "430", "origin": "third_country"},
            ],
        },
    });
    let invocation = bundle.parse_invocation(&invocation.to_string()).unwrap();
    let outcome = bundle.evaluate(&invocation).unwrap();

    assert_eq!(outcome.proves, "see_tco2e_per_t");
    assert_eq!(outcome.output.to_string(), "2.042323");
}

#[test]
fn rules_json_reproduces_the_overvoltage_method() {
    let files = [
        BundleFile {
            path: "rules.json",
            content: include_bytes!("../rules.json"),
        },
        BundleFile {
            path: "parameters/gwp.json",
            content: include_bytes!("../parameters/gwp.json"),
        },
    ];
    let bundle = RuleBundle::from_files(&files).unwrap();

    let invocation = serde_json::json!({
        "rule": "aluminium.primary.overvoltage",
        "context": {
            "sector": "aluminium",
            "route": "primary",
            "cnCode": "7601 10 00",
            "period": "2026",
        },
        "inputs": {
            "production_t": "100000",
            "direct_co2_t": "155000",
            "anode_effect_overvoltage_mv": "2.5",
            "current_efficiency_percent": "96",
            "overvoltage_coefficient_cf4": "1.16",
            "weight_fraction_c2f6_cf4": "0.121",
        },
    });
    let invocation = bundle.parse_invocation(&invocation.to_string()).unwrap();
    let outcome = bundle.evaluate(&invocation).unwrap();

    assert_eq!(outcome.proves, "see_tco2e_per_t");
    assert_eq!(
        outcome.outputs["cf4_t"].as_scalar().unwrap().to_string(),
        "3.020900"
    );
    assert_eq!(
        outcome.outputs["c2f6_t"].as_scalar().unwrap().to_string(),
        "0.365529"
    );
    assert_eq!(
        outcome.outputs["pfc_tco2e"]
            .as_scalar()
            .unwrap()
            .to_string(),
        "24085.938900"
    );
    assert_eq!(outcome.output.to_string(), "1.790859");
}

#[test]
fn rules_json_reproduces_the_appendix_b_vector() {
    let vector: Vector = serde_json::from_str(VECTOR).unwrap();

    let files = [
        BundleFile {
            path: "rules.json",
            content: include_bytes!("../rules.json"),
        },
        BundleFile {
            path: "parameters/gwp.json",
            content: include_bytes!("../parameters/gwp.json"),
        },
    ];
    let bundle = RuleBundle::from_files(&files).unwrap();

    let invocation = serde_json::json!({
        "rule": "aluminium.primary.slope",
        "context": {
            "sector": vector.sector,
            "route": vector.route,
            "cnCode": vector.cn_code,
            "period": vector.period,
        },
        "inputs": vector.inputs,
    });
    let invocation = bundle.parse_invocation(&invocation.to_string()).unwrap();
    let outcome = bundle.evaluate(&invocation).unwrap();

    assert_eq!(outcome.proves, "see_tco2e_per_t");
    for (output, key) in EXPECTED {
        let value = outcome.outputs[output].as_scalar().unwrap();
        assert_eq!(value.to_string(), vector.expected[key], "{output}");
    }
}
