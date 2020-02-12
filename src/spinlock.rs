// This implementation is based on:
// https://github.com/Amanieu/parking_lot/tree/fa294cd677936bf365afa0497039953b10c722f5/lock_api
// and
// https://github.com/mvdnes/spin-rs/tree/7516c8037d3d15712ba4d8499ab075e97a19d778

use lock_api::{RawMutex, GuardSend};
use core::sync::atomic::{AtomicBool, Ordering, spin_loop_hint};

/// Provides mutual exclusion based on spinning on an `AtomicBool`.
/// 
/// It's recommended to use this type either combination with [`lock_api::Mutex`] or
/// through the [`Spinlock`] type.
///
/// ## Example
/// 
/// ```rust
/// use lock_api::RawMutex;
/// let lock = spinning_top::RawSpinlock::INIT;
/// assert_eq!(lock.try_lock(), true); // lock it
/// assert_eq!(lock.try_lock(), false); // can't be locked a second time
/// lock.unlock(); // unlock it
/// assert_eq!(lock.try_lock(), true); // now it can be locked again
#[derive(Debug)]
pub struct RawSpinlock {
    /// Whether the spinlock is locked.
    locked: AtomicBool,
}

unsafe impl RawMutex for RawSpinlock {
    const INIT: RawSpinlock = RawSpinlock {
        locked: AtomicBool::new(false),
    };

    // A spinlock guard can be sent to another thread and unlocked there
    type GuardMarker = GuardSend;

    fn lock(&self) {
        while !self.try_lock() {
            // Wait until the lock looks unlocked before retrying
            // Code from https://github.com/mvdnes/spin-rs/commit/d3e60d19adbde8c8e9d3199c7c51e51ee5a20bf6
            while self.locked.load(Ordering::Relaxed)
            {
                // Tell the CPU that we're inside a busy-wait loop
                spin_loop_hint();
            }
        }
    }

    fn try_lock(&self) -> bool {
        self.locked.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok()
    }

    fn unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }
}

/// A mutual exclusion (Mutex) type based on busy-waiting.
/// 
/// Calling `lock` (or `try_lock`) on this type returns a [`SpinlockGuard`], which
/// automatically frees the lock when it goes out of scope.
///
/// ## Example
///
/// ```rust
/// use spinning_top::Spinlock;
/// 
/// fn main() {
///     // Wrap some data in a spinlock
///     let data = String::from("Hello");
///     let spinlock = Spinlock::new(data);
///     make_uppercase(&spinlock); // only pass a shared reference
///
///     // We have ownership of the spinlock, so we can extract the data without locking
///     // Note: this consumes the spinlock
///     let data = spinlock.into_inner();
///     assert_eq!(data.as_str(), "HELLO");
/// }
/// 
/// fn make_uppercase(spinlock: &Spinlock<String>) {
///     // Lock the spinlock to get a mutable reference to the data
///     let mut locked_data = spinlock.lock();
///     assert_eq!(locked_data.as_str(), "Hello");
///     locked_data.make_ascii_uppercase();
/// 
///     // the lock is automatically freed at the end of the scope
/// }
/// ```
/// 
/// ## Nightly Example
/// 
/// On Rust nightly, the `new` function is a `const` function, which makes the
/// `Spinlock` type usable in statics:
/// 
/// ```rust,ignore
/// use spinning_top::Spinlock;
/// 
/// static DATA: Spinlock<u32> = Spinlock::new(0);
/// 
/// fn main() {
///     let mut data = DATA.lock();
///     *data += 1;
///     assert_eq!(*data, 1);
/// }
/// ```
pub type Spinlock<T> = lock_api::Mutex<RawSpinlock, T>;

/// A RAII guard that frees the spinlock when it goes out of scope.
/// 
/// Allows access to the locked data through the [`core::ops::Deref`] and [`core::ops::DerefMut`] operations.
/// 
/// ## Example
/// 
/// ```rust
/// use spinning_top::{Spinlock, SpinlockGuard};
/// 
/// let spinlock = Spinlock::new(Vec::new());
/// 
/// // begin a new scope
/// { 
///     // lock the spinlock to create a `SpinlockGuard`
///     let mut guard: SpinlockGuard<_> = spinlock.lock();
/// 
///     // guard can be used like a `&mut Vec` since it implements `DerefMut`
///     guard.push(1);
///     guard.push(2);
///     assert_eq!(guard.len(), 2);
/// } // guard is dropped -> frees the spinlock again
/// 
/// // spinlock is unlocked again
/// assert!(spinlock.try_lock().is_some());
pub type SpinlockGuard<'a, T> = lock_api::MutexGuard<'a, RawSpinlock, T>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_lock() {
        let spinlock = Spinlock::new(42);
        let data = spinlock.try_lock();
        assert!(data.is_some());
        assert_eq!(*data.unwrap(), 42);
    }

    #[test]
    fn mutual_exclusion() {
        let spinlock = Spinlock::new(1);
        let data = spinlock.try_lock();
        assert!(data.is_some());
        assert!(spinlock.try_lock().is_none());
        assert!(spinlock.try_lock().is_none()); // still None
        core::mem::drop(data);
        assert!(spinlock.try_lock().is_some());
    }

    #[test]
    fn three_locks() {
        let spinlock1 = Spinlock::new(1);
        let spinlock2 = Spinlock::new(2);
        let spinlock3 = Spinlock::new(3);
        let data1 = spinlock1.try_lock();
        let data2 = spinlock2.try_lock();
        let data3 = spinlock3.try_lock();
        assert!(data1.is_some());
        assert!(data2.is_some());
        assert!(data3.is_some());
        assert!(spinlock1.try_lock().is_none());
        assert!(spinlock1.try_lock().is_none()); // still None
        assert!(spinlock2.try_lock().is_none());
        assert!(spinlock3.try_lock().is_none());
        core::mem::drop(data3);
        assert!(spinlock3.try_lock().is_some());
    }
}