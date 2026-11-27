//! A 2560x1440 SDR display for the supported Series X retail image (GTA VI
//! 1.0.2.1): the title presents at 3840x2160 on Series X, its scene drawn at
//! 2560x1440 and scaled up by its own temporal pass. With 1440p asked for
//! (`VICTORY_NATIVE_1440P=1`, or `VICTORY_RENDER_1080P=1` for a 1920x1080
//! scene scaled to 1440p), the image's SDR display mode, the scanout
//! reservation, the temporal scene scale and the Settings viewport load are
//! patched before it is mapped: the title then makes a 1440p output, and no
//! 4K pass runs.
//!
//! Ported from DexNom's fork (DexNom/VIctory, `apps/victory/src/native_1440p.rs`,
//! GPL-3.0-or-later), its native 1440p part only. Every field is checked
//! against the stock bytes first: another build is refused, not patched.

use anyhow::{Context, Result, ensure};
use victory_formats::texture::{Dimension, Swizzle, Texture, TextureDesc};
use victory_hle::streamline::Mode;
use victory_loader::PreparedImage;

const RETAIL_BUILD: &str = "b99f876b74d4a77f03af5f878223b27bd647b5c35a79d5decd9f716504239e93";
pub(super) const WIDTH: u32 = 2560;
pub(super) const HEIGHT: u32 = 1440;
const SDR_MODE: u32 = 0x0615_c100;
const STOCK_SDR_MODE: [u8; 16] = [0, 15, 0, 0, 112, 8, 0, 0, 60, 0, 0, 0, 1, 0, 0, 0];
const SCANOUT_RESERVATION: u32 = 0x0247_ad4a;
const TEMPORAL_SCALE: u32 = 0x028f_1e7e;
const UI_VIEWPORT_LOAD: u32 = 0x0580_48d4;
/// The interface's screen size (0x92d7924, width then height) is the
/// output's 2560x1440. The movie slots take their rectangle from it through
/// two getters (`mov eax, [global]; ret`: 0x4f21f10 the width, 0x4f21f00 the
/// height, through 0x4f21b90) and lay out in it with sizes made for
/// 3840x2160: the HUD came out 1.5 times too big. Those loads read the
/// legacy UI size (3840x2160 at 0xa9438ec) instead. The global itself stays
/// the output's: the map's terrain, its fog and what the cursor points at
/// are worked out from it, as the 4K output shows them. Each: the load
/// (`mov eax, [rip+disp]`), its stock target, the legacy one.
const UI_SCREEN_LOADS: [(u32, u32, u32); 2] = [
    (0x04f2_1f10, 0x092d_7924, 0x0a94_38ec),
    (0x04f2_1f00, 0x092d_7928, 0x0a94_38f0),
];
/// The map's cursor is placed on the interface by 0xb19260, which turns its
/// position (a fraction of the screen) into pixels with the screen size
/// (`GetScreenSize(&w, &h)`, `call [rax+0x30]` at 0xb19a4b): with the
/// output's size it sat at 2/3 of the centre. That call goes to a copy of
/// `GetScreenSize` (0xcab820) that answers the legacy size, in the int3
/// padding after 0x55ff283 (a `ret`; the next function starts at 0x55ff2c0).
const UI_MAP_CURSOR_CALL: u32 = 0x00b1_9a43;
const UI_LEGACY_SIZE_FN: u32 = 0x055f_f290;
const UI_LEGACY_SIZE: u32 = 0x0a94_38ec;
/// After snapping to a point of interest the cursor's position comes from
/// its own object (+0x140, pixels), which the interface is bound to: the
/// title sets it (0x23246d0) from a fraction times `GetScreenSizeF`
/// (0x22da840, called at 0x232478d) and reads it back as a fraction with
/// the same (0x2324351). Those two calls go to a copy that answers the
/// [3840.0, 2160.0] constant, in the int3 padding after the legacy size
/// copy, so that the bound position is the interface's. The four reads the
/// map's logic makes of it (what lies under the cursor 0x232415d, near it
/// 0x2324f1d, where it points in the world 0x2322fec, and the position
/// handed to the map's controller 0x23240fa) each call a stub that reads it
/// and scales it back by 2/3 to the output's pixels. The stubs live in the
/// int3 padding past the end of the code section's used size (0x5f40966,
/// filled out to its raw size, which is mapped with it).
const UI_SCREEN_SIZE_F: u32 = 0x022d_a840;
const UI_SCREEN_SIZE_F_LEGACY: u32 = 0x055f_f2b0;
const UI_CURSOR_SIZE_CALLS: [u32; 2] = [0x0232_4351, 0x0232_478d];
const UI_CURSOR_STUBS: u32 = 0x05f4_0970;
/// Each read: where, its 8 bytes (`vmovsd xmmN, [reg+0x140]`), and the
/// `vmulps xmmN, xmmN, [rip+disp]` opcode bytes that scale it.
const UI_CURSOR_READS: [(u32, [u8; 8], [u8; 4]); 4] = [
    (0x0232_415d, [0xc5, 0xfb, 0x10, 0xb1, 0x40, 0x01, 0, 0], [0xc5, 0xc8, 0x59, 0x35]),
    (0x0232_4f1d, [0xc5, 0xfb, 0x10, 0x87, 0x40, 0x01, 0, 0], [0xc5, 0xf8, 0x59, 0x05]),
    (0x0232_40fa, [0xc5, 0xfb, 0x10, 0x87, 0x40, 0x01, 0, 0], [0xc5, 0xf8, 0x59, 0x05]),
    (0x0232_2fec, [0xc5, 0xfb, 0x10, 0x86, 0x40, 0x01, 0, 0], [0xc5, 0xf8, 0x59, 0x05]),
];
/// Scaleform then gets 3840x2160 movie viewports in a 2560x1440 buffer
/// and leaves out what lies past it: 0x13cba40 builds the buffer size from
/// `vmovsd xmm0, [rsi+0xa0]`, which reads the [3840.0, 2160.0] at 0x615a3ec
/// instead. The GPU side puts those viewports and scissors onto the output
/// (`draw.rs`, `ui_canvas_scale`).
const UI_BUFFER_SIZE_LOAD: u32 = 0x013c_ba73;
const UI_CANVAS_SIZE: u32 = 0x0615_a3ec;
/// The menus (Settings > Controls' panels, its tabs) lay out from the size
/// of the target they are drawn for, read at 0x53915bc by
/// `vmovq xmm10, [rax+0x38]`: the output's 2560x1440 cut them to 2/3 of
/// their 3840x2160 layout. That read jumps to a cave in the int3 padding
/// after 0x55e8247 (a call that does not return; the next function starts
/// at 0x55e8280), which gives the legacy UI size for the output's.
const UI_TARGET_SIZE_LOAD: u32 = 0x0539_15bc;
const UI_TARGET_SIZE_CAVE: u32 = 0x055e_8250;

/// Patches `image` for a 2560x1440 output; the scene is drawn at 2560x1440,
/// or at 1920x1080 with `render_1080p`. Returns the scene's size.
pub(super) fn prepare(image: &mut PreparedImage, render_1080p: bool) -> Result<(u32, u32)> {
    ensure!(
        image.original_hash.to_hex().as_str() == RETAIL_BUILD,
        "1440p output is only known for the retail build {RETAIL_BUILD}"
    );

    let mode = field::<16>(image, SDR_MODE)?;
    ensure!(*mode == STOCK_SDR_MODE, "unexpected Series X SDR mode");
    mode[..4].copy_from_slice(&WIDTH.to_le_bytes());
    mode[4..8].copy_from_slice(&HEIGHT.to_le_bytes());

    // Two 64 KiB-aligned, tiled RGBA8 scanout buffers. The reservation is
    // rounded to 2 MiB, matching the title's allocation check.
    let texture = Texture::new(&TextureDesc {
        dimension: Dimension::Tex2d,
        width: WIDTH,
        height: HEIGHT,
        depth_or_array: 1,
        mips: 1,
        format: 28,
        samples: 1,
        swizzle: Swizzle::SW_64KB_D_X,
    })
    .map_err(|error| anyhow::anyhow!("1440p scanout layout: {error:?}"))?;
    let pool_size =
        u32::try_from((texture.size.next_multiple_of(64 * 1024) * 2).next_multiple_of(2 << 20))?;
    let reservation = field::<5>(image, SCANOUT_RESERVATION)?;
    ensure!(*reservation == [0xb9, 0, 0, 0, 4], "unexpected Series X scanout reservation");
    reservation[1..].copy_from_slice(&pool_size.to_le_bytes());

    let scale = field::<4>(image, TEMPORAL_SCALE)?;
    ensure!(*scale == (2.0f32 / 3.0).to_le_bytes(), "unexpected Series X temporal scene scale");
    let (render_width, render_height) = if render_1080p { (1920, 1080) } else { (WIDTH, HEIGHT) };
    *scale = (render_width as f32 / WIDTH as f32).to_le_bytes();

    // `VICTORY_UI_SCREEN=0` leaves the interface on the output's size.
    let ui_screen = std::env::var("VICTORY_UI_SCREEN").as_deref() != Ok("0");

    // Settings lays out its preview panels with a viewport setter of its
    // own that loads the legacy UI size (3840x2160) even when a 1440p UI
    // surface was made: it reads the selected SDR mode instead.
    let viewport = field::<8>(image, UI_VIEWPORT_LOAD)?;
    ensure!(
        *viewport == [0xc5, 0xfb, 0x10, 0x0d, 0x10, 0xf0, 0x13, 0x05],
        "unexpected Settings viewport size load"
    );
    let displacement = (SDR_MODE as i32) - (UI_VIEWPORT_LOAD as i32 + 8);
    viewport[4..].copy_from_slice(&displacement.to_le_bytes());

    if ui_screen {
        let rel = |end: u32, to: u32| (to.wrapping_sub(end) as i32).to_le_bytes();
        for (at, stock, legacy) in UI_SCREEN_LOADS {
            let load = field::<6>(image, at)?;
            let disp = i32::from_le_bytes(load[2..].try_into()?);
            ensure!(
                load[..2] == [0x8b, 0x05] && (at + 6).wrapping_add_signed(disp) == stock,
                "unexpected interface screen size load"
            );
            load[2..].copy_from_slice(&rel(at + 6, legacy));
        }
        ensure!(
            *field::<8>(image, UI_CANVAS_SIZE)? == [0, 0, 0x70, 0x45, 0, 0, 0x07, 0x45],
            "unexpected [3840.0, 2160.0] constant"
        );
        let load = field::<8>(image, UI_BUFFER_SIZE_LOAD)?;
        ensure!(*load == [0xc5, 0xfb, 0x10, 0x86, 0xa0, 0, 0, 0], "unexpected viewport buffer size load");
        load[3] = 0x05; // modrm: xmm0, [rip+disp32]
        load[4..].copy_from_slice(&(UI_CANVAS_SIZE.wrapping_sub(UI_BUFFER_SIZE_LOAD + 8) as i32).to_le_bytes());

        // mov rax, [r13]; vaddps xmm6, xmm0, xmm1; call [rax+0x30] becomes
        // vaddps xmm6, xmm0, xmm1; call legacy size; xchg ax, ax
        let call = field::<11>(image, UI_MAP_CURSOR_CALL)?;
        ensure!(
            *call == [0x49, 0x8b, 0x45, 0x00, 0xc5, 0xf8, 0x58, 0xf1, 0xff, 0x50, 0x30],
            "unexpected map cursor size call"
        );
        call[..4].copy_from_slice(&[0xc5, 0xf8, 0x58, 0xf1]);
        call[4] = 0xe8;
        call[5..9].copy_from_slice(&rel(UI_MAP_CURSOR_CALL + 9, UI_LEGACY_SIZE_FN));
        call[9..].copy_from_slice(&[0x66, 0x90]);
        let mut sizer = Vec::with_capacity(18);
        sizer.extend_from_slice(&[0x8b, 0x05]); // mov eax, [rip+legacy width]
        sizer.extend_from_slice(&rel(UI_LEGACY_SIZE_FN + 6, UI_LEGACY_SIZE));
        sizer.extend_from_slice(&[0x89, 0x02, 0x8b, 0x05]); // mov [rdx], eax; mov eax, [rip+legacy height]
        sizer.extend_from_slice(&rel(UI_LEGACY_SIZE_FN + 14, UI_LEGACY_SIZE + 4));
        sizer.extend_from_slice(&[0x41, 0x89, 0x00, 0xc3]); // mov [r8], eax; ret
        let free = field::<18>(image, UI_LEGACY_SIZE_FN)?;
        ensure!(free.iter().all(|&b| b == 0xcc), "unexpected padding for the legacy size function");
        free.copy_from_slice(&sizer);

        // GetScreenSizeF's copy: mov rax, [rip+[3840.0, 2160.0]]; mov [rdx], rax; ret
        let mut sizer_f = vec![0x48, 0x8b, 0x05];
        sizer_f.extend_from_slice(&rel(UI_SCREEN_SIZE_F_LEGACY + 7, UI_CANVAS_SIZE));
        sizer_f.extend_from_slice(&[0x48, 0x89, 0x02, 0xc3]);
        let free = field::<11>(image, UI_SCREEN_SIZE_F_LEGACY)?;
        ensure!(free.iter().all(|&b| b == 0xcc), "unexpected padding for the legacy float size function");
        free.copy_from_slice(&sizer_f);
        for at in UI_CURSOR_SIZE_CALLS {
            let call = field::<5>(image, at)?;
            ensure!(
                call[0] == 0xe8 && call[1..] == rel(at + 5, UI_SCREEN_SIZE_F),
                "unexpected map cursor size call"
            );
            call[1..].copy_from_slice(&rel(at + 5, UI_SCREEN_SIZE_F_LEGACY));
        }
        // [2/3, 2/3, 0, 0], then one stub per distinct read:
        // vmovsd xmmN, [reg+0x140]; vmulps xmmN, xmmN, [scale]; ret
        let mut stubs: Vec<u8> = [2560.0f32 / 3840.0, 1440.0 / 2160.0, 0.0, 0.0].iter().flat_map(|f| f.to_le_bytes()).collect();
        let mut entries: Vec<([u8; 8], u32)> = Vec::new();
        for (_, load, scale) in UI_CURSOR_READS {
            if entries.iter().any(|(l, _)| *l == load) {
                continue;
            }
            let at = UI_CURSOR_STUBS + stubs.len() as u32;
            entries.push((load, at));
            stubs.extend_from_slice(&load);
            stubs.extend_from_slice(&scale);
            stubs.extend_from_slice(&rel(at + 16, UI_CURSOR_STUBS));
            stubs.push(0xc3);
        }
        let free = section_tail(image, UI_CURSOR_STUBS, stubs.len())?;
        ensure!(free.iter().all(|&b| b == 0xcc), "unexpected padding for the map cursor stubs");
        free.copy_from_slice(&stubs);
        for (at, load, _) in UI_CURSOR_READS {
            let stub = entries.iter().find(|(l, _)| *l == load).map(|(_, a)| *a).context("map cursor stub")?;
            let read = field::<8>(image, at)?;
            ensure!(*read == load, "unexpected map cursor position read");
            read[0] = 0xe8;
            read[1..5].copy_from_slice(&rel(at + 5, stub));
            read[5..].copy_from_slice(&[0x0f, 0x1f, 0x00]);
        }

        // The size read becomes a jump into the cave, which reads it, and
        // takes the legacy one instead when it is the output's.
        let back = UI_TARGET_SIZE_LOAD + 5;
        let mut cave = Vec::with_capacity(44);
        cave.extend_from_slice(&[0xc5, 0x7a, 0x7e, 0x50, 0x38]); // vmovq xmm10, [rax+0x38]
        cave.extend_from_slice(&[0x81, 0x78, 0x38]); // cmp dword [rax+0x38], WIDTH
        cave.extend_from_slice(&WIDTH.to_le_bytes());
        cave.extend_from_slice(&[0x0f, 0x85]); // jne back
        cave.extend_from_slice(&rel(UI_TARGET_SIZE_CAVE + 18, back));
        cave.extend_from_slice(&[0x81, 0x78, 0x3c]); // cmp dword [rax+0x3c], HEIGHT
        cave.extend_from_slice(&HEIGHT.to_le_bytes());
        cave.extend_from_slice(&[0x0f, 0x85]); // jne back
        cave.extend_from_slice(&rel(UI_TARGET_SIZE_CAVE + 31, back));
        cave.extend_from_slice(&[0xc5, 0x7a, 0x7e, 0x15]); // vmovq xmm10, [rip+legacy size]
        cave.extend_from_slice(&rel(UI_TARGET_SIZE_CAVE + 39, UI_LEGACY_SIZE));
        cave.push(0xe9); // jmp back
        cave.extend_from_slice(&rel(UI_TARGET_SIZE_CAVE + 44, back));
        let free = field::<44>(image, UI_TARGET_SIZE_CAVE)?;
        ensure!(free.iter().all(|&b| b == 0xcc), "unexpected padding for the interface size cave");
        free.copy_from_slice(&cave);
        let load = field::<5>(image, UI_TARGET_SIZE_LOAD)?;
        ensure!(*load == [0xc5, 0x7a, 0x7e, 0x50, 0x38], "unexpected interface target size load");
        load[0] = 0xe9;
        load[1..].copy_from_slice(&rel(back, UI_TARGET_SIZE_CAVE));
    }
    Ok((render_width, render_height))
}

/// The title's renderer selection: `mov eax, 6; cmovne eax, ecx` (the
/// title's own temporal pass, or on Series S with the upscaler there the
/// FidelityFX path, 7).
const RENDERER_SELECTION: u32 = 0x007e_9421;
const STOCK_RENDERER_SELECTION: [u8; 8] = [0xb8, 6, 0, 0, 0, 0x0f, 0x45, 0xc1];
/// The FidelityFX path's render size: the console check's result (Series S)
/// is noted as the FidelityFX quality mode and picks the scaled branch
/// (`movzx eax, al; mov [r14+0x90], eax; test al, al; jne`), whose scale
/// multiplies the output size.
const UPSCALER_SIZING: u32 = 0x028e_f69a;
const STOCK_UPSCALER_SIZING: [u8; 14] = [0x0f, 0xb6, 0xc0, 0x41, 0x89, 0x86, 0x90, 0, 0, 0, 0x84, 0xc0, 0x75, 0x0a];
/// The native branch's load of 1.0, and the scaled branch's of the scale.
const NATIVE_LOAD: u32 = 0x028e_f6a8;
const STOCK_NATIVE_LOAD: [u8; 10] = [0xc5, 0xfa, 0x10, 0x05, 0x28, 0x78, 0x6d, 0x03, 0xeb, 0x08];
const SCALED_LOAD: u32 = 0x028e_f6b2;
const STOCK_SCALED_LOAD: [u8; 8] = [0xc5, 0xfa, 0x10, 0x05, 0xa2, 0xe2, 0x86, 0x03];
/// The scale the scaled branch loads (shared with other code: not written).
const UPSCALER_SCALE: u32 = 0x0615_d95c;

/// Makes the title render through its FidelityFX upscaler path (renderer 7,
/// as Series S does) with the scene at `mode`'s scale of the output, so that
/// DLSS can take its upscale dispatches over (`victory_hle::modules::fidelityfx`).
/// On Series X the stock title uses its own temporal pass instead and never
/// loads its upscaler. The FidelityFX quality mode the title notes is
/// `mode`'s; the temporal scene scale (the title's own pass, and
/// `prepare`'s 1440p patch) is set to the same scale, as the stock Series X
/// image pairs them (2/3 each). Every patched instruction is checked against
/// the stock bytes first; nothing is written if one differs.
///
/// From DexNom's fork (DexNom/VIctory, `apps/victory/src/native_1440p.rs`,
/// `prepare_fsr_4k`, GPL-3.0-or-later), there for a 4K output; kept here
/// with the 1440p one.
pub(super) fn prepare_upscaler(image: &mut PreparedImage, mode: Mode) -> Result<()> {
    ensure!(
        image.original_hash.to_hex().as_str() == RETAIL_BUILD,
        "the upscaler path is only known for the retail build {RETAIL_BUILD}"
    );
    ensure!(*field::<8>(image, RENDERER_SELECTION)? == STOCK_RENDERER_SELECTION, "unexpected renderer selection");
    ensure!(*field::<14>(image, UPSCALER_SIZING)? == STOCK_UPSCALER_SIZING, "unexpected upscaler render size selection");
    ensure!(*field::<10>(image, NATIVE_LOAD)? == STOCK_NATIVE_LOAD, "unexpected upscaler native scale load");
    ensure!(*field::<8>(image, SCALED_LOAD)? == STOCK_SCALED_LOAD, "unexpected upscaler scaled load");
    ensure!(*field::<4>(image, UPSCALER_SCALE)? == (2.0f32 / 3.0).to_le_bytes(), "unexpected upscaler scale");
    let temporal = f32::from_le_bytes(*field::<4>(image, TEMPORAL_SCALE)?);
    ensure!(
        temporal == 2.0 / 3.0 || temporal == 1.0,
        "unexpected temporal scene scale {temporal} (stock 2/3, or 1 with the 1440p output)"
    );
    // Renderer 7 every time it is asked (the console check has run).
    *field::<8>(image, RENDERER_SELECTION)? = [0xb8, 7, 0, 0, 0, 0x90, 0x90, 0x90];
    // `mov eax, mode; mov [r14+0x90], eax; jmp scaled`.
    *field::<14>(image, UPSCALER_SIZING)? = [0xb8, mode.ffx_mode(), 0, 0, 0, 0x41, 0x89, 0x86, 0x90, 0, 0, 0, 0xeb, 0x0a];
    if mode != Mode::Quality {
        // The jump leaves the native branch unreached: its load's operand
        // becomes a literal of ours, which the scaled branch loads.
        let literal = NATIVE_LOAD + 4;
        *field::<4>(image, literal)? = mode.scale().to_le_bytes();
        let displacement = literal as i32 - (SCALED_LOAD as i32 + 8);
        *field::<4>(image, SCALED_LOAD + 4)? = displacement.to_le_bytes();
    }
    *field::<4>(image, TEMPORAL_SCALE)? = mode.scale().to_le_bytes();
    Ok(())
}

/// `len` bytes at `rva` within a section's raw data, past its used size
/// too (`field` stops there): the padding the file maps with it.
fn section_tail(image: &mut PreparedImage, rva: u32, len: usize) -> Result<&mut [u8]> {
    let offset = image
        .info
        .sections
        .iter()
        .find_map(|s| {
            let delta = rva.checked_sub(s.rva)?;
            (delta as usize + len <= s.file_size as usize).then(|| s.file_offset as usize + delta as usize)
        })
        .context("section padding missing")?;
    image.bytes.get_mut(offset..offset + len).context("incomplete section padding")
}

fn field<const N: usize>(image: &mut PreparedImage, rva: u32) -> Result<&mut [u8; N]> {
    let offset = image.info.rva_to_file_offset(rva).context("display profile field missing")? as usize;
    image
        .bytes
        .get_mut(offset..offset + N)
        .context("incomplete display profile field")?
        .try_into()
        .context("invalid display profile field length")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With the retail executable named by `VICTORY_DISPLAY_PROFILE_TEST_IMAGE`
    /// (not part of the repository): the four fields hold what a 1440p
    /// output needs, for both scene sizes; without it, nothing is checked.
    #[test]
    fn patches_the_retail_image_for_1440p() {
        let Some(path) = std::env::var_os("VICTORY_DISPLAY_PROFILE_TEST_IMAGE") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        for (render_1080p, scene) in [(false, (2560, 1440)), (true, (1920, 1080))] {
            let mut image = victory_loader::prepare(&bytes).unwrap();
            assert_eq!(prepare(&mut image, render_1080p).unwrap(), scene);
            let mode = *field::<16>(&mut image, SDR_MODE).unwrap();
            assert_eq!(mode[..4], WIDTH.to_le_bytes());
            assert_eq!(mode[4..8], HEIGHT.to_le_bytes());
            let scale = f32::from_le_bytes(*field::<4>(&mut image, TEMPORAL_SCALE).unwrap());
            assert_eq!((WIDTH as f32 * scale) as u32, scene.0);
            let load = *field::<8>(&mut image, UI_VIEWPORT_LOAD).unwrap();
            let disp = i32::from_le_bytes(load[4..].try_into().unwrap());
            assert_eq!(UI_VIEWPORT_LOAD.wrapping_add(8).wrapping_add_signed(disp), SDR_MODE);
            assert_eq!(field::<1>(&mut image, UI_TARGET_SIZE_LOAD).unwrap()[0], 0xe9);
            assert_eq!(field::<1>(&mut image, UI_MAP_CURSOR_CALL + 4).unwrap()[0], 0xe8);
            for (at, _, _) in UI_CURSOR_READS {
                assert_eq!(field::<1>(&mut image, at).unwrap()[0], 0xe8);
            }
            assert_eq!(section_tail(&mut image, UI_CURSOR_STUBS + 16, 1).unwrap()[0], 0xc5);
            for (at, _, legacy) in UI_SCREEN_LOADS {
                let disp = i32::from_le_bytes(field::<6>(&mut image, at).unwrap()[2..].try_into().unwrap());
                assert_eq!((at + 6).wrapping_add_signed(disp), legacy);
            }
            // Patched once, the image is no longer the stock one: refused.
            assert!(prepare(&mut image, render_1080p).is_err());
        }
    }

    /// With the retail executable (as above): the upscaler path at each
    /// scale, alone (4K output) and after the 1440p patch; the scaled
    /// branch then loads the mode's scale and the noted mode is its
    /// FidelityFX mode.
    #[test]
    fn patches_the_upscaler_path() {
        let Some(path) = std::env::var_os("VICTORY_DISPLAY_PROFILE_TEST_IMAGE") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        for native in [false, true] {
            for mode in [Mode::Dlaa, Mode::Quality, Mode::Balanced, Mode::Performance, Mode::UltraPerformance] {
                let mut image = victory_loader::prepare(&bytes).unwrap();
                if native {
                    prepare(&mut image, false).unwrap();
                }
                prepare_upscaler(&mut image, mode).unwrap();
                assert_eq!(*field::<8>(&mut image, RENDERER_SELECTION).unwrap(), [0xb8, 7, 0, 0, 0, 0x90, 0x90, 0x90]);
                let sizing = *field::<14>(&mut image, UPSCALER_SIZING).unwrap();
                assert_eq!(sizing[1], mode.ffx_mode());
                assert_eq!(sizing[12..], [0xeb, 0x0a]);
                // The jump lands on the scaled load.
                assert_eq!(UPSCALER_SIZING + 14 + 0x0a, SCALED_LOAD);
                let disp = i32::from_le_bytes(*field::<4>(&mut image, SCALED_LOAD + 4).unwrap());
                let loaded = (SCALED_LOAD + 8).wrapping_add_signed(disp);
                assert_eq!(f32::from_le_bytes(*field::<4>(&mut image, loaded).unwrap()), mode.scale());
                assert_eq!(f32::from_le_bytes(*field::<4>(&mut image, TEMPORAL_SCALE).unwrap()), mode.scale());
                // The shared constant stays as it was.
                assert_eq!(*field::<4>(&mut image, UPSCALER_SCALE).unwrap(), (2.0f32 / 3.0).to_le_bytes());
                // Patched once, refused.
                assert!(prepare_upscaler(&mut image, mode).is_err());
            }
        }
    }
}
