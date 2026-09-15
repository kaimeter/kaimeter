//! Conformance test for the worked example of whitepaper §4.5 and Appendix B.
//!
//! The crate's copy of the vector must stay byte-identical to the published
//! one, and the primary-route evaluation must reproduce every expected value
//! at the bundle's full fixed-point precision.

use std::collections::BTreeMap;

use kaimeter_rules::fixed::Fixed;
use kaimeter_rules::parameters::ParameterTables;
use kaimeter_rules::sectors::aluminium::boundaries::see_primary_slope;
use kaimeter_rules::sectors::aluminium::pfc::{Gwp, SlopeInput};
use serde::Deserialize;

/// The crate's conformance copy of the published vector.
const VECTOR: &str = include_str!("vectors/aluminium/worked-example.json");

/// The published vector under `paper/vectors/`.
const PAPER_VECTOR: &str = include_str!("../../../paper/vectors/worked-example-aluminium.json");

/// The Appendix B vector.
#[derive(Deserialize)]
struct Vector {
    bundle: String,
    sector: String,
    route: String,
    cn_code: String,
    period: String,
    method: String,
    parameters: BTreeMap<String, String>,
    inputs: BTreeMap<String, String>,
    expected: BTreeMap<String, String>,
}

fn fixed(values: &BTreeMap<String, String>, key: &str) -> Fixed {
    values[key].parse().unwrap()
}

#[test]
fn conformance_copy_matches_the_published_vector() {
    assert_eq!(VECTOR, PAPER_VECTOR);
}

#[test]
fn appendix_b_vector_passes_at_full_precision() {
    let vector: Vector = serde_json::from_str(VECTOR).unwrap();
    assert_eq!(vector.bundle, "cbam");
    assert_eq!(vector.sector, "aluminium");
    assert_eq!(vector.route, "primary");
    assert_eq!(vector.cn_code, "7601");
    assert_eq!(vector.period, "2026");
    assert_eq!(vector.method, "pfc_slope");

    let tables = ParameterTables::embedded().unwrap();
    let gwp = Gwp::from_table(&tables.gwp).unwrap();
    for (substance, key) in [("CF4", "gwp_cf4"), ("C2F6", "gwp_c2f6")] {
        let row = tables
            .gwp
            .rows
            .iter()
            .find(|row| row.substance == substance)
            .unwrap();
        assert_eq!(row.gwp100, vector.parameters[key]);
    }

    let input = SlopeInput {
        anode_effect_minutes_per_cell_day: fixed(
            &vector.inputs,
            "anode_effect_minutes_per_cell_day",
        ),
        slope_emission_factor_cf4: fixed(&vector.inputs, "slope_emission_factor_cf4"),
        weight_fraction_c2f6_cf4: fixed(&vector.inputs, "weight_fraction_c2f6_cf4"),
        production_t: fixed(&vector.inputs, "production_t"),
    };
    let direct_co2_t = fixed(&vector.inputs, "direct_co2_t");
    let emissions = see_primary_slope(&input, direct_co2_t, &gwp).unwrap();

    assert_eq!(emissions.cf4_t, fixed(&vector.expected, "e_cf4_t"));
    assert_eq!(emissions.c2f6_t, fixed(&vector.expected, "e_c2f6_t"));
    assert_eq!(emissions.pfc_tco2e, fixed(&vector.expected, "e_pfc_tco2e"));
    assert_eq!(
        emissions.see_tco2e_per_t,
        fixed(&vector.expected, "see_tco2e_per_t")
    );

    // The published strings are the six-decimal display of the same values.
    assert_eq!(emissions.cf4_t.to_string(), vector.expected["e_cf4_t"]);
    assert_eq!(emissions.c2f6_t.to_string(), vector.expected["e_c2f6_t"]);
    assert_eq!(
        emissions.pfc_tco2e.to_string(),
        vector.expected["e_pfc_tco2e"]
    );
    assert_eq!(
        emissions.see_tco2e_per_t.to_string(),
        vector.expected["see_tco2e_per_t"]
    );
}
