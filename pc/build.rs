// The PC build: cfg(pc) tells the shared code (../src) it has a PC's
// memory to work with, so its caches can be larger than on the board (up
// to about 60 MB). Not for 32-bit machines, such as routers, which keep
// the board's small caches; nor with MINETEST_SMALL_CACHES=1 set.
fn main() {
    println!("cargo:rustc-check-cfg=cfg(pc)");
    println!("cargo:rerun-if-env-changed=MINETEST_SMALL_CACHES");
    let wide = std::env::var("CARGO_CFG_TARGET_POINTER_WIDTH").as_deref() == Ok("64");
    if wide && std::env::var_os("MINETEST_SMALL_CACHES").is_none() {
        println!("cargo:rustc-cfg=pc");
    }
}
