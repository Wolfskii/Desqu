//! Per-module named lock so only one process (standalone app or Desqu) runs a module's engine.

use crate::platform::{self, LockHandle};

/// Held while the module runs; dropping it lets another process take over.
pub struct ModuleLock {
    _handle: LockHandle,
}

pub fn lock_name(module_id: &str) -> String {
    format!(r"Local\Desqu.Module.{module_id}")
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// `Ok(None)` means another process already runs this module.
pub fn try_acquire(module_id: &str) -> Result<Option<ModuleLock>, String> {
    if !valid_id(module_id) {
        return Err(format!("invalid module id '{module_id}'"));
    }
    Ok(platform::acquire_named_lock(&lock_name(module_id))?.map(|h| ModuleLock { _handle: h }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique(tag: &str) -> String {
        format!("test-{tag}-{}", std::process::id())
    }

    #[test]
    fn name_is_session_local_and_namespaced() {
        assert_eq!(lock_name("screenbound"), r"Local\Desqu.Module.screenbound");
    }

    #[test]
    fn rejects_odd_ids() {
        assert!(try_acquire("").is_err());
        assert!(try_acquire("a b").is_err());
        assert!(try_acquire(r"a\b").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn second_acquire_fails_until_the_first_is_dropped() {
        let id = unique("twice");
        let first = try_acquire(&id).unwrap();
        assert!(first.is_some());
        assert!(try_acquire(&id).unwrap().is_none());
        drop(first);
        assert!(try_acquire(&id).unwrap().is_some());
    }

    #[cfg(windows)]
    #[test]
    fn different_modules_do_not_block_each_other() {
        let a = try_acquire(&unique("a")).unwrap();
        let b = try_acquire(&unique("b")).unwrap();
        assert!(a.is_some() && b.is_some());
    }

    #[cfg(windows)]
    #[test]
    fn can_be_released_from_another_thread() {
        let id = unique("thread");
        let lock = try_acquire(&id).unwrap().unwrap();
        std::thread::spawn(move || drop(lock)).join().unwrap();
        assert!(try_acquire(&id).unwrap().is_some());
    }
}
