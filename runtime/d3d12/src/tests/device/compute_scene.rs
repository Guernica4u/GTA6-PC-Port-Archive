use super::*;

fn scene(space: u32, register: u32) -> Binding {
    Binding { class: 4, space, lower: register, upper: register, kind: 16 }
}

fn raw(space: u32, register: u32) -> Binding {
    Binding { class: 4, space, lower: register, upper: register, kind: 11 }
}

fn texture(space: u32, register: u32) -> Binding {
    Binding { class: 3, space, lower: register, upper: register, kind: 2 }
}

#[test]
fn space_one_layout_keeps_its_single_scene_and_geometry() {
    let guest = [texture(0, 0), scene(0, 5)];
    let layout = inline_layout(&guest, &[scene(1, 0), texture(0, 0)]).unwrap();
    assert_eq!(layout.space, 1);
    assert_eq!(layout.scenes, [(0, (0, 5))]);
    assert_eq!(layout.private_registers(), [0]);
    let geometry = inline_layout(&guest, &[scene(1, 0), raw(1, 1), raw(1, 2)]).unwrap();
    assert!(geometry.triangle_geometry);
    assert_eq!(geometry.private_registers(), [0, 1, 2]);
    // Half the geometry tables, or a title binding in space 1, is not this ABI.
    assert_eq!(inline_layout(&guest, &[scene(1, 0), raw(1, 1)]), None);
    assert_eq!(inline_layout(&[scene(0, 5), texture(1, 3)], &[scene(1, 0)]), None);
    assert_eq!(inline_layout(&[scene(0, 5), scene(0, 6)], &[scene(1, 0)]), None);
}

#[test]
fn private_space_maps_each_register_to_the_titles_ordered_scenes() {
    // A title using space 1 for everything, with near and far scenes.
    let guest = [texture(1, 373), scene(1, 356), scene(1, 355), raw(1, 10)];
    let both = inline_layout(&guest, &[scene(SCENE_SPACE, 1), scene(SCENE_SPACE, 0), texture(1, 373)]).unwrap();
    assert_eq!(both.space, SCENE_SPACE);
    assert_eq!(both.scenes, [(0, (1, 355)), (1, (1, 356))]);
    assert_eq!(both.private_registers(), [0, 1]);
    // DXC drops an unread scene, so a translation may declare only the far one.
    let far = inline_layout(&guest, &[scene(SCENE_SPACE, 1)]).unwrap();
    assert_eq!(far.scenes, [(1, (1, 356))]);
    assert_eq!(far.private_registers(), [1]);
}

#[test]
fn private_space_refuses_what_it_cannot_map() {
    let guest = [scene(1, 355)];
    // No title scene for register 1, a non-scene private SRV, an array, or a
    // title that already uses the private space.
    assert_eq!(inline_layout(&guest, &[scene(SCENE_SPACE, 1)]), None);
    assert_eq!(inline_layout(&guest, &[raw(SCENE_SPACE, 0)]), None);
    let array = Binding { upper: 1, ..scene(SCENE_SPACE, 0) };
    assert_eq!(inline_layout(&guest, &[array]), None);
    assert_eq!(inline_layout(&[scene(1, 355), texture(SCENE_SPACE, 0)], &[scene(SCENE_SPACE, 0)]), None);
    let title_array = Binding { upper: 356, ..scene(1, 355) };
    assert_eq!(inline_layout(&[title_array], &[scene(SCENE_SPACE, 0)]), None);
}
