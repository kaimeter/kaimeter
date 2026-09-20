//! Pinned guest image ID (interpreter contract §2).
//!
//! The image ID changes whenever the guest program or its toolchain changes,
//! so any such change must regenerate this constant in the same commit:
//! `cargo risczero build --manifest-path crates/kaimeter-guest/guest/Cargo.toml`
//! prints the ImageID for the current source.

use kaimeter_guest_host::KAIMETER_GUEST_ID;

/// Image ID of the guest program at bundle `2026.3.0`.
const PINNED_IMAGE_ID: [u32; 8] = [
    3681400802, 3296324264, 419619577, 1872511805, 188036292, 859864869, 4057132774, 1655794617,
];

#[test]
fn guest_image_id_matches_the_pin() {
    assert_eq!(KAIMETER_GUEST_ID, PINNED_IMAGE_ID);
}
