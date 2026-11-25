//! Every unit class from data/units.json has a sprite entry and every
//! referenced sprite file exists.

use std::path::PathBuf;

#[path = "../src/assets_check.rs"]
#[allow(dead_code)]
mod assets_check;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn sprites_cover_units_and_files_exist() {
    let r = root();
    let problems = assets_check::check_sprites(&r.join("data"), &r.join("assets")).unwrap();
    assert!(problems.is_empty(), "{problems:#?}");
}
