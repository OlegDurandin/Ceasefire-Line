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

/// R-UI-06: a bound texture missing from the manifest is an error; a manifest
/// texture bound nowhere is only a warning.
#[test]
fn r_ui_06_texture_bindings_against_the_manifest() {
    let sprites: assets_check::SpritesFile = serde_json::from_str(
        r#"{"sprite_sides": {}, "terrain": {"plain": "t_plain"},
            "biome_terrain": {"steppe": {"plain": "t_steppe"}},
            "units": {}, "helicopter_transfer": {}, "map_objects": {}}"#,
    )
    .unwrap();
    let (problems, warnings) =
        assets_check::texture_bindings(&sprites, ["t_plain", "t_spare"].into_iter());
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("t_steppe"));
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("t_spare"));
}
