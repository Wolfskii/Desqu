//! `cargo run --release --example crash_probe`: crashes on purpose to show what the crash report looks like.
fn main() {
    desqu_native_shell::crash::install();
    // SAFETY: deliberately invalid, to produce an access violation.
    unsafe { std::ptr::write_volatile(8 as *mut u32, 1) };
}
