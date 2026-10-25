use kobayashi::data::hostile::{load_hostile_index, DEFAULT_HOSTILES_INDEX_PATH};
use kobayashi::data::loader::resolve_hostile_with_index;
use std::path::Path;

#[test]
fn differing_hostile_variants_require_an_explicit_id() {
    let index = load_hostile_index(DEFAULT_HOSTILES_INDEX_PATH).unwrap();
    let dir = Path::new(DEFAULT_HOSTILES_INDEX_PATH).parent().unwrap();
    for name in ["Federation Patrol 35", "federation_patrol_35"] {
        assert!(
            resolve_hostile_with_index(&index, dir, name).is_none(),
            "ambiguous name must not silently select a variant: {name}"
        );
    }
    let explorer = resolve_hostile_with_index(&index, dir, "3752796").unwrap();
    let interceptor = resolve_hostile_with_index(&index, dir, "1135194818").unwrap();
    assert_ne!(explorer.ship_class, interceptor.ship_class);
    assert_eq!(explorer.id, "3752796");
    assert_eq!(interceptor.id, "1135194818");
}
