use super::*;

#[test]
fn coalesces_only_same_timeline_and_keeps_first_order() {
    let mut waits = Vec::new();
    for (fence, tick) in [(1, 4), (2, 8), (1, 2), (3, 5), (1, 7), (2, 9)] {
        include(&mut waits, &fence, tick);
    }
    assert_eq!(waits, [(1, 7), (2, 9), (3, 5)]);
}

#[test]
fn caller_filters_blocked_dependencies_before_coalescing() {
    let mut waits = Vec::new();
    for (fence, tick, blocked) in [(1, 3, false), (1, 5, true), (2, 7, false)] {
        if !blocked { include(&mut waits, &fence, tick); }
    }
    assert_eq!(waits, [(1, 3), (2, 7)]);
}

#[test]
fn shared_texture_dependencies_scale_with_queues_not_texture_count() {
    let mut waits = Vec::new();
    for texture in 0..8192 {
        for fence in 0..3 {
            include(&mut waits, &fence, texture + 1);
        }
    }
    assert_eq!(waits, [(0, 8192), (1, 8192), (2, 8192)]);
}
