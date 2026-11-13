use std::io;
use std::sync::{Mutex, MutexGuard};

/// A thread that panicked while holding the state does not take the host down with it
/// (plan.md principle 4): the lock is cleared and the state served as the panic left it.
pub(super) fn lock<T>(m: &Mutex<T>) -> io::Result<MutexGuard<'_, T>> {
    Ok(m.lock().unwrap_or_else(|poisoned| {
        eprintln!("state lock poisoned by a panicked thread; clearing it and serving on");
        m.clear_poison();
        poisoned.into_inner()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_under_the_lock_leaves_it_usable() {
        let m = Mutex::new(1);
        let _ = std::panic::catch_unwind(|| {
            let _held = m.lock().unwrap();
            panic!("handler bug");
        });
        assert!(m.is_poisoned());
        *lock(&m).unwrap() += 1;
        assert!(!m.is_poisoned());
        assert_eq!(*lock(&m).unwrap(), 2);
    }
}
