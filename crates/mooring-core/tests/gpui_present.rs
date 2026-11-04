//! The window's normal frame must not use a Metal blit encoder. The first blit in a
//! process makes the driver hold another 20 blocks of 8 MB (160 MB on an M3 Max) while
//! frames keep coming, which no Metal counter or CPU budget shows. See
//! docs/performance/index.html.
use std::path::Path;

#[test]
fn a_presented_frame_uses_no_blit_encoder() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.deps/zed/crates/gpui_apple/src/metal_renderer.rs");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}; run bootstrap.sh", path.display()));
    let start = source.find("fn draw_retained(").expect("draw_retained");
    let end = start
        + source[start..]
            .find("fn verify_retained(")
            .expect("verify_retained");
    assert!(
        !source[start..end].contains("new_blit_command_encoder"),
        "draw_retained encodes a blit: present with a render pass instead"
    );
}
