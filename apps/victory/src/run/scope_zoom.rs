//! Read the title's telescopic view before routing mouse-wheel zoom.

use super::lockon::active_action_state;

// The action hash table at RVA 0x0619_1240 and the native
// INPUTGROUP_SNIPER_ZOOM_SECONDARY control listed in the optics help.
pub(super) const ZOOM_IN: usize = 0x052;
pub(super) const ZOOM_OUT: usize = 0x053;

// At 0x017c_139e, the HUD tests bit 0 of this field before calling its
// TelescopicSight renderer at 0x0290_09a0. The render reader selects the
// completed snapshot at 0x017b_d890. The producer collects the live request
// and clears it at 0x0162_f808, so that request may already be clear at input time.
const RENDER_SNAPSHOT_INDEX: usize = 0x0a21_0d14;
const RENDER_SNAPSHOTS: usize = 0x0918_8cd0;
const SNAPSHOT_STRIDE: usize = 0x11e50;
const SIGHT_FLAGS: usize = 0x11028;

const OTHER_ACTIONS: [usize; 17] = [
    0x018, // INPUT_SELECT_NEXT_WHEEL
    0x024, // INPUT_SELECT_NEXT_WEAPON
    0x025, // INPUT_SELECT_PREV_WEAPON
    0x033, // INPUT_ATTACK
    0x034, // INPUT_AIM
    0x04a, // INPUT_OPEN_WHEEL_MENU
    0x04c, // INPUT_QUICK_SWAP_WEAPON
    0x135, // INPUT_MELEE_ATTACK
    0x16a, // INPUT_MAP
    0x17c, // INPUT_FRONTEND_DOWN
    0x17d, // INPUT_FRONTEND_UP
    0x180, // INPUT_FRONTEND_RDOWN
    0x181, // INPUT_FRONTEND_RUP
    0x1ad, // INPUT_GAME_MENU_UP
    0x1ae, // INPUT_GAME_MENU_DOWN
    0x1d2, // INPUT_PREV_WEAPON
    0x1d3, // INPUT_NEXT_WEAPON
];

unsafe fn sight_visible(index: *const u32, snapshots: *const u8) -> bool {
    // SAFETY: the fixed-title caller supplies the HUD's mapped snapshot array.
    let slot = unsafe { index.read_volatile() };
    if slot > 1 {
        return false;
    }
    let offset = slot as usize * SNAPSHOT_STRIDE + SIGHT_FLAGS;
    // The reader index can change while the renderer takes the next buffer.
    let flags = unsafe { snapshots.add(offset).read_volatile() };
    flags & 1 != 0 && unsafe { index.read_volatile() } == slot
}

/// # Safety
/// The loader must have verified the Scarlett 1.0.2.1 image. The player
/// comes from the current primary on-foot gameplay-pad check.
pub(super) unsafe fn scope_active(base: usize, player: *const u8) -> bool {
    if base == 0 || player.is_null() {
        return false;
    }
    unsafe {
        sight_visible(
            (base + RENDER_SNAPSHOT_INDEX) as *const u32,
            (base + RENDER_SNAPSHOTS) as *const u8,
        )
    }
}

fn exclusive_zoom_row(
    id: usize,
    mut resolve: impl FnMut(usize) -> Option<*const u8>,
) -> Option<*const u8> {
    let other_direction = match id {
        ZOOM_IN => ZOOM_OUT,
        ZOOM_OUT => ZOOM_IN,
        _ => return None,
    };
    let row = resolve(id)?;
    if resolve(other_direction) == Some(row)
        || OTHER_ACTIONS
            .into_iter()
            .any(|other| resolve(other) == Some(row))
    {
        return None;
    }
    Some(row)
}

/// # Safety
/// The base must reference the verified title image and its live primary pad.
/// The caller checks the optics context before publishing the wheel sample.
pub(super) unsafe fn zoom_action_state(base: usize, id: usize) -> Option<*const u8> {
    exclusive_zoom_row(id, |action| unsafe { active_action_state(base, action) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sight_uses_the_displayed_snapshot_and_its_visibility_bit() {
        let mut snapshots = vec![0u8; SNAPSHOT_STRIDE + SIGHT_FLAGS + 1];
        snapshots[SIGHT_FLAGS] = 1;
        snapshots[SNAPSHOT_STRIDE + SIGHT_FLAGS] = 2;
        assert!(unsafe { sight_visible(&0, snapshots.as_ptr()) });
        assert!(!unsafe { sight_visible(&1, snapshots.as_ptr()) });
        snapshots[SNAPSHOT_STRIDE + SIGHT_FLAGS] = 3;
        assert!(unsafe { sight_visible(&1, snapshots.as_ptr()) });
        assert!(!unsafe { sight_visible(&2, snapshots.as_ptr()) });
        assert!(!unsafe { scope_active(0, std::ptr::null()) });
    }

    fn resolve(bindings: &[(usize, *const u8)], action: usize) -> Option<*const u8> {
        bindings
            .iter()
            .find_map(|&(id, row)| (id == action).then_some(row))
    }

    #[test]
    fn routes_only_the_requested_and_eligible_zoom_direction() {
        let (up, down) = (1u8, 2u8);
        let bindings = [(ZOOM_IN, &raw const up), (ZOOM_OUT, &raw const down)];
        assert_eq!(
            exclusive_zoom_row(ZOOM_IN, |id| resolve(&bindings, id)),
            Some(&raw const up)
        );
        assert_eq!(
            exclusive_zoom_row(ZOOM_OUT, |id| resolve(&bindings, id)),
            Some(&raw const down)
        );
        assert_eq!(exclusive_zoom_row(ZOOM_IN, |_| None), None);
        assert_eq!(
            exclusive_zoom_row(0x018, |_| panic!("resolved a non-zoom action")),
            None
        );
    }

    #[test]
    fn rejects_zoom_rows_shared_with_wheel_map_menus_or_the_opposite_direction() {
        let up = 1u8;
        for other in [ZOOM_OUT].into_iter().chain(OTHER_ACTIONS) {
            let bindings = [(ZOOM_IN, &raw const up), (other, &raw const up)];
            assert_eq!(
                exclusive_zoom_row(ZOOM_IN, |id| resolve(&bindings, id)),
                None
            );
        }
    }
}
