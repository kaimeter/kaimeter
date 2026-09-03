// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 Keldrion, LLC and contributors

//! Unit normalization: kWh ↔ MWh and kg ↔ tonnes.
//!
//! Conversions are exact power-of-ten scalings over `f64`; round-trips on
//! ordinary magnitudes do not lose precision (pinned by tests).

/// Kilowatt-hours per megawatt-hour.
pub const KWH_PER_MWH: f64 = 1000.0;

/// Kilograms per metric tonne.
pub const KG_PER_TONNE: f64 = 1000.0;

/// Convert kilowatt-hours to megawatt-hours.
#[must_use]
pub fn kwh_to_mwh(kwh: f64) -> f64 {
    kwh / KWH_PER_MWH
}

/// Convert megawatt-hours to kilowatt-hours.
#[must_use]
pub fn mwh_to_kwh(mwh: f64) -> f64 {
    mwh * KWH_PER_MWH
}

/// Convert kilograms to metric tonnes.
#[must_use]
pub fn kg_to_tonnes(kg: f64) -> f64 {
    kg / KG_PER_TONNE
}

/// Convert metric tonnes to kilograms.
#[must_use]
pub fn tonnes_to_kg(tonnes: f64) -> f64 {
    tonnes * KG_PER_TONNE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_one_thousand() {
        assert_eq!(KWH_PER_MWH, 1000.0);
        assert_eq!(KG_PER_TONNE, 1000.0);
    }

    #[test]
    fn conversions_use_exact_powers_of_ten() {
        assert_eq!(kwh_to_mwh(1000.0), 1.0);
        assert_eq!(mwh_to_kwh(1.0), 1000.0);
        assert_eq!(kg_to_tonnes(1000.0), 1.0);
        assert_eq!(tonnes_to_kg(1.0), 1000.0);
    }

    #[test]
    fn round_trips_are_lossless_on_representative_magnitudes() {
        for kwh in [0.0, 1.0, 2.5, 999.0, 4_200.0, 123_456.0, 1e9] {
            assert_eq!(mwh_to_kwh(kwh_to_mwh(kwh)), kwh);
        }
        for kg in [0.0, 0.5, 3.25, 987_654.0, 1e9] {
            assert_eq!(tonnes_to_kg(kg_to_tonnes(kg)), kg);
        }
    }
}
