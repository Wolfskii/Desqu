//! Keeps an app that has no window from being slowed down by Windows.
//!
//! Windows 11 treats a process with no visible window as background work: its threads run on the
//! efficient cores and its timers can be held back by a second or more. That is fine for most
//! programs and wrong for these: Screenbound moves a window in steps of 16 ms, Workspaces has to
//! swap the desktop icons the moment you switch desktops. Under Desqu the apps run without a window,
//! so each one opts out at start.

/// Asks Windows not to throttle this process (execution speed) and to honour its timer requests.
/// Does nothing where Windows does not know the setting.
#[cfg(windows)]
pub fn stay_responsive() {
    use windows::Win32::System::Threading::{
        GetCurrentProcess, ProcessPowerThrottling, SetProcessInformation, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION, PROCESS_POWER_THROTTLING_STATE,
    };
    // A control bit with its state bit left at 0 turns that throttling off.
    let state = PROCESS_POWER_THROTTLING_STATE {
        Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED | PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION,
        StateMask: 0,
    };
    // SAFETY: the state is a plain struct that outlives the call; the handle is this process.
    let result = unsafe {
        SetProcessInformation(GetCurrentProcess(), ProcessPowerThrottling, &state as *const _ as *const std::ffi::c_void, std::mem::size_of_val(&state) as u32)
    };
    if let Err(e) = result {
        eprintln!("could not turn off power throttling: {e}");
    }
}

#[cfg(not(windows))]
pub fn stay_responsive() {}
