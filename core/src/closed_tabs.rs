//! Bounded LIFO of recently closed tabs.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

const CAPACITY: usize = 25;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ClosedTab {
    pub url: String,
    pub title: String,
    pub pinned: bool,
    pub index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ClosedTabs {
    stack: VecDeque<ClosedTab>,
}

impl ClosedTabs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, tab: ClosedTab) {
        self.stack.push_back(tab);
        while self.stack.len() > CAPACITY {
            self.stack.pop_front();
        }
    }

    pub fn pop(&mut self) -> Option<ClosedTab> {
        self.stack.pop_back()
    }

    pub fn len(&self) -> usize {
        self.stack.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    /// Newest first.
    pub fn list(&self) -> Vec<ClosedTab> {
        self.stack.iter().rev().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(i: usize) -> ClosedTab {
        ClosedTab {
            url: format!("https://e.com/{i}"),
            title: String::new(),
            pinned: false,
            index: i,
        }
    }

    #[test]
    fn lifo_and_bounded() {
        let mut c = ClosedTabs::new();
        assert!(c.is_empty());
        assert!(c.pop().is_none());
        for i in 0..30 {
            c.push(tab(i));
        }
        assert_eq!(c.len(), 25);
        assert_eq!(c.list()[0].index, 29);
        assert_eq!(c.list()[24].index, 5);
        assert_eq!(c.pop().unwrap().index, 29);
        assert_eq!(c.len(), 24);
    }
}
