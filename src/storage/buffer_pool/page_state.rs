//! Page state machine for buffer pool management.
//!
//! Pages transition through states: EVICTED → LOCKED → MARKED → UNLOCKED → EVICTED
//!
//! This is a simplified version of `KuzuDB`'s page state machine, optimized for
//! correctness over performance in this MVP implementation.

use std::sync::atomic::{AtomicU64, Ordering};

/// Possible states for a page in the buffer pool.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageStateValue {
    /// Page is available for optimistic reads.
    Unlocked = 0,
    /// Page is exclusively locked for modification.
    Locked = 1,
    /// Page is in the eviction queue but can be rescued.
    Marked = 2,
    /// Page is not in memory.
    Evicted = 3,
}

impl TryFrom<u8> for PageStateValue {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(PageStateValue::Unlocked),
            1 => Ok(PageStateValue::Locked),
            2 => Ok(PageStateValue::Marked),
            3 => Ok(PageStateValue::Evicted),
            _ => Err(()),
        }
    }
}

/// Atomic page state with version counter for optimistic reads.
///
/// The state is packed into a 64-bit integer:
/// - Bits 63: dirty flag
/// - Bits 56-62: state value (`PageStateValue`)
/// - Bits 0-55: version counter for optimistic concurrency
pub struct PageState {
    /// Packed state: [dirty:1][state:7][version:56]
    state: AtomicU64,
}

const DIRTY_SHIFT: u64 = 63;
const STATE_SHIFT: u64 = 56;
const DIRTY_MASK: u64 = 1 << DIRTY_SHIFT;
const STATE_MASK: u64 = 0x7F << STATE_SHIFT;
const VERSION_MASK: u64 = (1 << STATE_SHIFT) - 1;

impl PageState {
    /// Creates a new page state in the EVICTED state.
    #[must_use]
    pub fn new() -> Self {
        let initial = (PageStateValue::Evicted as u64) << STATE_SHIFT;
        Self {
            state: AtomicU64::new(initial),
        }
    }

    /// Creates a new page state with the given initial state.
    #[must_use]
    pub fn with_state(state: PageStateValue) -> Self {
        let initial = (state as u64) << STATE_SHIFT;
        Self {
            state: AtomicU64::new(initial),
        }
    }

    /// Returns the current state value.
    #[must_use]
    pub fn get_state(&self) -> PageStateValue {
        let packed = self.state.load(Ordering::Acquire);
        let state_bits = ((packed & STATE_MASK) >> STATE_SHIFT) as u8;
        PageStateValue::try_from(state_bits).unwrap_or(PageStateValue::Evicted)
    }

    /// Returns whether the page is dirty.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        let packed = self.state.load(Ordering::Acquire);
        (packed & DIRTY_MASK) != 0
    }

    /// Returns the current version number.
    #[must_use]
    pub fn get_version(&self) -> u64 {
        let packed = self.state.load(Ordering::Acquire);
        packed & VERSION_MASK
    }

    /// Sets the dirty flag.
    pub fn set_dirty(&self, dirty: bool) {
        loop {
            let old = self.state.load(Ordering::Acquire);
            let new = if dirty {
                old | DIRTY_MASK
            } else {
                old & !DIRTY_MASK
            };
            if self
                .state
                .compare_exchange(old, new, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }
    }

    /// Attempts to transition from EVICTED to LOCKED (pin operation).
    ///
    /// Returns `true` if the transition succeeded.
    pub fn try_lock_from_evicted(&self) -> bool {
        let old = self.state.load(Ordering::Acquire);
        let old_state = ((old & STATE_MASK) >> STATE_SHIFT) as u8;

        if PageStateValue::try_from(old_state) != Ok(PageStateValue::Evicted) {
            return false;
        }

        let version = old & VERSION_MASK;
        let new = (PageStateValue::Locked as u64) << STATE_SHIFT | (version + 1);

        self.state
            .compare_exchange(old, new, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
    }

    /// Attempts to transition from any pinnable state to LOCKED.
    ///
    /// Returns `true` if the transition succeeded.
    pub fn try_lock(&self) -> bool {
        loop {
            let old = self.state.load(Ordering::Acquire);
            let old_state = ((old & STATE_MASK) >> STATE_SHIFT) as u8;

            match PageStateValue::try_from(old_state) {
                Ok(PageStateValue::Unlocked | PageStateValue::Marked) => {
                    let dirty = old & DIRTY_MASK;
                    let version = old & VERSION_MASK;
                    let new =
                        dirty | (PageStateValue::Locked as u64) << STATE_SHIFT | (version + 1);

                    if self
                        .state
                        .compare_exchange(old, new, Ordering::AcqRel, Ordering::Relaxed)
                        .is_ok()
                    {
                        return true;
                    }
                    // CAS failed, retry
                }
                Ok(PageStateValue::Locked) => return false, // Already locked
                Ok(PageStateValue::Evicted) | Err(()) => return false, // Can't lock evicted page
            }
        }
    }

    /// Transitions from LOCKED to MARKED (unpin operation).
    ///
    /// # Panics
    ///
    /// Panics if the current state is not LOCKED.
    pub fn unlock_to_marked(&self) {
        loop {
            let old = self.state.load(Ordering::Acquire);
            let dirty = old & DIRTY_MASK;
            let version = old & VERSION_MASK;
            let new = dirty | (PageStateValue::Marked as u64) << STATE_SHIFT | version;

            if self
                .state
                .compare_exchange(old, new, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }
    }

    /// Transitions from MARKED to UNLOCKED (access during eviction scan).
    pub fn mark_to_unlocked(&self) -> bool {
        let old = self.state.load(Ordering::Acquire);
        let old_state = ((old & STATE_MASK) >> STATE_SHIFT) as u8;

        if PageStateValue::try_from(old_state) != Ok(PageStateValue::Marked) {
            return false;
        }

        let dirty = old & DIRTY_MASK;
        let version = old & VERSION_MASK;
        let new = dirty | (PageStateValue::Unlocked as u64) << STATE_SHIFT | version;

        self.state
            .compare_exchange(old, new, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
    }

    /// Transitions to EVICTED (eviction operation).
    ///
    /// Returns `true` if the transition succeeded (page was MARKED or UNLOCKED).
    pub fn try_evict(&self) -> bool {
        let old = self.state.load(Ordering::Acquire);
        let old_state = ((old & STATE_MASK) >> STATE_SHIFT) as u8;

        match PageStateValue::try_from(old_state) {
            Ok(PageStateValue::Marked | PageStateValue::Unlocked) => {
                let version = old & VERSION_MASK;
                let new = (PageStateValue::Evicted as u64) << STATE_SHIFT | (version + 1);

                self.state
                    .compare_exchange(old, new, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
            }
            _ => false,
        }
    }

    /// Resets the state to EVICTED with cleared dirty flag.
    pub fn reset(&self) {
        let old = self.state.load(Ordering::Acquire);
        let version = old & VERSION_MASK;
        let new = (PageStateValue::Evicted as u64) << STATE_SHIFT | (version + 1);
        self.state.store(new, Ordering::Release);
    }
}

impl Default for PageState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let state = PageState::new();
        assert_eq!(state.get_state(), PageStateValue::Evicted);
        assert!(!state.is_dirty());
        assert_eq!(state.get_version(), 0);
    }

    #[test]
    fn test_lock_from_evicted() {
        let state = PageState::new();
        assert!(state.try_lock_from_evicted());
        assert_eq!(state.get_state(), PageStateValue::Locked);
        assert_eq!(state.get_version(), 1);
    }

    #[test]
    fn test_unlock_to_marked() {
        let state = PageState::with_state(PageStateValue::Locked);
        state.unlock_to_marked();
        assert_eq!(state.get_state(), PageStateValue::Marked);
    }

    #[test]
    fn test_mark_to_unlocked() {
        let state = PageState::with_state(PageStateValue::Marked);
        assert!(state.mark_to_unlocked());
        assert_eq!(state.get_state(), PageStateValue::Unlocked);
    }

    #[test]
    fn test_dirty_flag() {
        let state = PageState::new();
        assert!(!state.is_dirty());
        state.set_dirty(true);
        assert!(state.is_dirty());
        state.set_dirty(false);
        assert!(!state.is_dirty());
    }

    #[test]
    fn test_try_evict() {
        let state = PageState::with_state(PageStateValue::Marked);
        assert!(state.try_evict());
        assert_eq!(state.get_state(), PageStateValue::Evicted);
    }

    #[test]
    fn test_cannot_evict_locked() {
        let state = PageState::with_state(PageStateValue::Locked);
        assert!(!state.try_evict());
        assert_eq!(state.get_state(), PageStateValue::Locked);
    }

    #[test]
    fn test_full_lifecycle() {
        let state = PageState::new();

        // EVICTED -> LOCKED (pin)
        assert!(state.try_lock_from_evicted());
        assert_eq!(state.get_state(), PageStateValue::Locked);

        // LOCKED -> MARKED (unpin)
        state.unlock_to_marked();
        assert_eq!(state.get_state(), PageStateValue::Marked);

        // MARKED -> UNLOCKED (access during eviction scan)
        assert!(state.mark_to_unlocked());
        assert_eq!(state.get_state(), PageStateValue::Unlocked);

        // UNLOCKED -> EVICTED (evict)
        assert!(state.try_evict());
        assert_eq!(state.get_state(), PageStateValue::Evicted);
    }
}
