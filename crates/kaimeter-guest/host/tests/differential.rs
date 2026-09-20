//! Native/guest differential tests (interpreter contract §6).
//!
//! Every conformance vector and a batch of seeded random invocation records
//! must produce identical results in three executions: the typed plain
//! evaluator, the interpreter run natively, and the RISC Zero guest. A
//! disagreement fails with the offending invocation and case name printed.

use kaimeter_guest_host::KAIMETER_GUEST_ELF;
use kaimeter_interpreter::bundle::{bundle_hash, BundleFile};
use kaimeter_interpreter::rule::RuleBundle;
use kaimeter_rules::common::precursors::{see_complex, PrecursorOrigin, PrecursorSupply};
use kaimeter_rules::fixed::{Fixed, SCALE};
use kaimeter_rules::parameters::ParameterTables;
use kaimeter_rules::sectors::aluminium::boundaries::{
    see_primary_overvoltage, see_primary_slope, see_secondary,
};
use kaimeter_rules::sectors::aluminium::pfc::{Gwp, OvervoltageInput, SlopeInput};
use risc0_zkvm::{default_executor, ExecutorEnv};

/// The committed rule encoding, frozen at compile time.
const RULES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../kaimeter-rules/rules.json"
));

/// The committed GWP table, frozen at compile time.
const GWP: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../kaimeter-rules/parameters/gwp.json"
));

/// The published two-supplier vector of whitepaper §11.
const TWO_SUPPLIER: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../kaimeter-rules/tests/vectors/aluminium/two-supplier-precursors.json"
));

/// Seed of the random batch; a failure is reproducible from this value.
const SEED: u64 = 0x5EED_2026;

/// Number of random invocation records.
const RANDOM_CASES: usize = 24;

fn fixed(text: &str) -> Fixed {
    text.parse().unwrap()
}

/// A deterministic splitmix64 generator.
struct Rng(u64);

impl Rng {
    /// Returns a value in `[min, max)` scaled by [`SCALE`].
    fn between(&mut self, min: i128, max: i128) -> Fixed {
        let span = (max - min) as u64;
        Fixed::from_scaled(min + (self.next() % span) as i128)
    }

    /// Returns one step of the sequence.
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// The GWP table of the committed bundle.
fn gwp() -> Gwp {
    Gwp::from_table(&ParameterTables::embedded().unwrap().gwp).unwrap()
}

/// Public context for one invocation.
fn context(route: &str, cn_code: &str) -> serde_json::Value {
    serde_json::json!({
        "sector": "aluminium",
        "route": route,
        "cnCode": cn_code,
        "period": "2026",
    })
}

/// The witness file set shared by every execution.
fn bundle_files() -> [BundleFile<'static>; 2] {
    [
        BundleFile {
            path: "rules.json",
            content: RULES,
        },
        BundleFile {
            path: "parameters/gwp.json",
            content: GWP,
        },
    ]
}

/// Evaluates the interpreter natively and returns its output and journal.
fn native_journal(invocation_text: &str) -> (String, Vec<u8>) {
    let files = bundle_files();
    let bundle = RuleBundle::from_files(&files).unwrap();
    let hash = bundle.bundle_hash().unwrap();
    let invocation = bundle.parse_invocation(invocation_text).unwrap();
    let outcome = bundle.evaluate(&invocation).unwrap();
    let output = outcome.output.to_string();
    (
        output,
        outcome.journal(hash, invocation.context).canonical_bytes(),
    )
}

/// Executes the guest and returns its committed journal.
fn guest_journal(invocation_text: &str) -> Vec<u8> {
    let files = bundle_files();
    let hash = bundle_hash(&files).unwrap();
    let file_count = files.len() as u32;
    let claimed = hash.as_bytes().to_vec();
    let invocation = invocation_text.to_string();
    let mut builder = ExecutorEnv::builder();
    builder.write(&file_count).unwrap();
    for file in &files {
        builder.write(&file.path.to_string()).unwrap();
        builder.write(&file.content.to_vec()).unwrap();
    }
    builder.write(&invocation).unwrap();
    builder.write(&claimed).unwrap();
    let env = builder.build().unwrap();
    default_executor()
        .execute(env, KAIMETER_GUEST_ELF)
        .unwrap()
        .journal
        .bytes
}

/// Checks one invocation in all three executions.
fn assert_agreement(case: &str, invocation: serde_json::Value, typed: String) {
    let text = invocation.to_string();
    let (native, expected) = native_journal(&text);
    assert_eq!(typed, native, "{case}: typed and interpreter disagree");
    let committed = guest_journal(&text);
    assert_eq!(
        committed, expected,
        "{case}: guest and interpreter disagree"
    );
}

/// Returns typed supplies and their JSON form.
fn random_supplies(rng: &mut Rng) -> (Vec<PrecursorSupply>, serde_json::Value) {
    let count = 1 + (rng.next() % 3) as usize;
    let mut typed = Vec::with_capacity(count);
    let mut json = Vec::with_capacity(count);
    for _ in 0..count {
        let see = rng.between(0, 5 * SCALE);
        let quantity = rng.between(SCALE / 10, 1_000 * SCALE);
        let origin = if rng.next() % 3 == 0 {
            PrecursorOrigin::EuOrExcluded
        } else {
            PrecursorOrigin::ThirdCountry
        };
        let origin_text = match origin {
            PrecursorOrigin::ThirdCountry => "third_country",
            PrecursorOrigin::EuOrExcluded => "eu_or_excluded",
        };
        typed.push(PrecursorSupply {
            see,
            quantity_t: quantity,
            origin,
        });
        json.push(serde_json::json!({
            "see_tco2e_per_t": see.to_string(),
            "quantity_t": quantity.to_string(),
            "origin": origin_text,
        }));
    }
    (typed, serde_json::Value::Array(json))
}

#[test]
fn conformance_vectors_agree_across_executions() {
    let gwp = gwp();

    let input = OvervoltageInput {
        anode_effect_overvoltage_mv: fixed("2.5"),
        current_efficiency_percent: fixed("96"),
        overvoltage_coefficient_cf4: fixed("1.16"),
        weight_fraction_c2f6_cf4: fixed("0.121"),
        production_t: fixed("100000"),
    };
    let typed = see_primary_overvoltage(&input, fixed("155000"), &gwp)
        .unwrap()
        .see_tco2e_per_t
        .to_string();
    assert_eq!(typed, "1.790859");
    assert_agreement(
        "overvoltage",
        serde_json::json!({
            "rule": "aluminium.primary.overvoltage",
            "context": context("primary", "7601 10 00"),
            "inputs": {
                "production_t": "100000",
                "direct_co2_t": "155000",
                "anode_effect_overvoltage_mv": "2.5",
                "current_efficiency_percent": "96",
                "overvoltage_coefficient_cf4": "1.16",
                "weight_fraction_c2f6_cf4": "0.121",
            },
        }),
        typed,
    );

    let supplies = [
        PrecursorSupply {
            see: fixed("1.9"),
            quantity_t: fixed("100"),
            origin: PrecursorOrigin::ThirdCountry,
        },
        PrecursorSupply {
            see: fixed("9.9"),
            quantity_t: fixed("30"),
            origin: PrecursorOrigin::EuOrExcluded,
        },
    ];
    let typed = see_secondary(fixed("40"), fixed("950"), &supplies)
        .unwrap()
        .to_string();
    assert_eq!(typed, "0.242105");
    assert_agreement(
        "secondary",
        serde_json::json!({
            "rule": "aluminium.secondary.see",
            "context": context("secondary", "7601"),
            "inputs": {
                "production_t": "950",
                "direct_co2_t": "40",
                "supplies": [
                    {"see_tco2e_per_t": "1.9", "quantity_t": "100", "origin": "third_country"},
                    {"see_tco2e_per_t": "9.9", "quantity_t": "30", "origin": "eu_or_excluded"},
                ],
            },
        }),
        typed,
    );

    let vector: serde_json::Value = serde_json::from_str(TWO_SUPPLIER).unwrap();
    let typed_supplies: Vec<PrecursorSupply> = vector["supplies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|supply| PrecursorSupply {
            see: fixed(supply["see_tco2e_per_t"].as_str().unwrap()),
            quantity_t: fixed(supply["quantity_t"].as_str().unwrap()),
            origin: match supply["origin"].as_str().unwrap() {
                "third_country" => PrecursorOrigin::ThirdCountry,
                _ => PrecursorOrigin::EuOrExcluded,
            },
        })
        .collect();
    let typed = see_complex(
        fixed(vector["inputs"]["direct_tco2e_t"].as_str().unwrap()),
        fixed(vector["inputs"]["production_t"].as_str().unwrap()),
        &typed_supplies,
    )
    .unwrap()
    .to_string();
    assert_eq!(typed, "2.042323");
    assert_agreement(
        "two-supplier",
        serde_json::json!({
            "rule": "common.complex.see",
            "context": context("complex", "7604 10 90"),
            "inputs": {
                "production_t": vector["inputs"]["production_t"],
                "direct_tco2e_t": vector["inputs"]["direct_tco2e_t"],
                "supplies": vector["supplies"],
            },
        }),
        typed,
    );
}

#[test]
fn seeded_random_inputs_agree_across_executions() {
    let gwp = gwp();
    let mut rng = Rng(SEED);
    for case in 0..RANDOM_CASES {
        match case % 4 {
            0 => {
                let input = SlopeInput {
                    anode_effect_minutes_per_cell_day: rng.between(0, 2 * SCALE),
                    slope_emission_factor_cf4: rng.between(SCALE / 20, SCALE / 2),
                    weight_fraction_c2f6_cf4: rng.between(SCALE / 20, SCALE / 3),
                    production_t: rng.between(1_000 * SCALE, 200_000 * SCALE),
                };
                let direct = rng.between(0, 500_000 * SCALE);
                let typed = see_primary_slope(&input, direct, &gwp)
                    .unwrap()
                    .see_tco2e_per_t
                    .to_string();
                assert_agreement(
                    &format!("random slope #{case}"),
                    serde_json::json!({
                        "rule": "aluminium.primary.slope",
                        "context": context("primary", "7601"),
                        "inputs": {
                            "production_t": input.production_t.to_string(),
                            "direct_co2_t": direct.to_string(),
                            "anode_effect_minutes_per_cell_day": input
                                .anode_effect_minutes_per_cell_day
                                .to_string(),
                            "slope_emission_factor_cf4": input.slope_emission_factor_cf4.to_string(),
                            "weight_fraction_c2f6_cf4": input.weight_fraction_c2f6_cf4.to_string(),
                        },
                    }),
                    typed,
                );
            }
            1 => {
                let input = OvervoltageInput {
                    anode_effect_overvoltage_mv: rng.between(SCALE / 10, 10 * SCALE),
                    current_efficiency_percent: rng.between(80 * SCALE, 100 * SCALE),
                    overvoltage_coefficient_cf4: rng.between(SCALE / 2, 4 * SCALE),
                    weight_fraction_c2f6_cf4: rng.between(SCALE / 20, SCALE / 3),
                    production_t: rng.between(1_000 * SCALE, 200_000 * SCALE),
                };
                let direct = rng.between(0, 500_000 * SCALE);
                let typed = see_primary_overvoltage(&input, direct, &gwp)
                    .unwrap()
                    .see_tco2e_per_t
                    .to_string();
                assert_agreement(
                    &format!("random overvoltage #{case}"),
                    serde_json::json!({
                        "rule": "aluminium.primary.overvoltage",
                        "context": context("primary", "7601"),
                        "inputs": {
                            "production_t": input.production_t.to_string(),
                            "direct_co2_t": direct.to_string(),
                            "anode_effect_overvoltage_mv": input
                                .anode_effect_overvoltage_mv
                                .to_string(),
                            "current_efficiency_percent": input
                                .current_efficiency_percent
                                .to_string(),
                            "overvoltage_coefficient_cf4": input
                                .overvoltage_coefficient_cf4
                                .to_string(),
                            "weight_fraction_c2f6_cf4": input.weight_fraction_c2f6_cf4.to_string(),
                        },
                    }),
                    typed,
                );
            }
            _ => {
                let production = rng.between(1_000 * SCALE, 100_000 * SCALE);
                let direct = rng.between(0, 10_000 * SCALE);
                let (typed_supplies, supplies) = random_supplies(&mut rng);
                let (rule, typed) = if case % 4 == 2 {
                    (
                        "aluminium.secondary.see",
                        see_secondary(direct, production, &typed_supplies)
                            .unwrap()
                            .to_string(),
                    )
                } else {
                    (
                        "common.complex.see",
                        see_complex(direct, production, &typed_supplies)
                            .unwrap()
                            .to_string(),
                    )
                };
                let direct_name = if case % 4 == 2 {
                    "direct_co2_t"
                } else {
                    "direct_tco2e_t"
                };
                let mut inputs = serde_json::Map::new();
                inputs.insert(
                    "production_t".to_string(),
                    serde_json::json!(production.to_string()),
                );
                inputs.insert(
                    direct_name.to_string(),
                    serde_json::json!(direct.to_string()),
                );
                inputs.insert("supplies".to_string(), supplies);
                assert_agreement(
                    &format!("random {rule} #{case}"),
                    serde_json::json!({
                        "rule": rule,
                        "context": context("complex", "7604 10 90"),
                        "inputs": serde_json::Value::Object(inputs),
                    }),
                    typed,
                );
            }
        }
    }
}
