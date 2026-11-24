//! A short history of what the user did, for undo. Pure.

/// The last few states, oldest dropped first.
#[derive(Debug)]
pub struct History<T> {
    states: Vec<T>,
    cap: usize,
}

impl<T> History<T> {
    pub fn new(cap: usize) -> Self {
        Self { states: Vec::new(), cap: cap.max(1) }
    }

    /// Remembers the state from before a change.
    pub fn push(&mut self, state: T) {
        if self.states.len() == self.cap {
            self.states.remove(0);
        }
        self.states.push(state);
    }

    /// The state to go back to, and forgets it.
    pub fn pop(&mut self) -> Option<T> {
        self.states.pop()
    }

    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_come_back_newest_first_and_the_oldest_fall_off() {
        let mut h = History::new(3);
        assert!(h.is_empty() && h.pop().is_none());
        for n in 1..=5 {
            h.push(n);
        }
        assert_eq!((h.pop(), h.pop(), h.pop(), h.pop()), (Some(5), Some(4), Some(3), None));
    }
}
