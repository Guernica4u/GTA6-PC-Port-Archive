//! Emit the synthetic loop-around-the-exchange compute DS_SWIZZLE_B32 program.
use tuxbox_xshader::{hlsl, ShaderExtract, Stage};

mod fixture {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/support/compute_swizzle_fixture.rs"));
}

fn main() {
    let offset = std::env::args().nth(1).expect("swizzle offset");
    let offset = u32::from_str_radix(offset.trim_start_matches("0x"), 16).expect("hexadecimal offset");
    assert!(offset <= 0xffff);
    let translated = hlsl::translate(&fixture::shader_loop_exit_countdown(offset))
        .expect("translate synthetic loop swizzle");
    print!("{}", translated.hlsl);
}
