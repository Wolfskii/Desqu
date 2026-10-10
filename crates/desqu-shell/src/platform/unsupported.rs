//! Stubs for platforms without a backend.

pub fn remove_run_value(_name: &str) -> Result<bool, String> {
    Ok(false)
}
