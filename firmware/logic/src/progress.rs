#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Feed,
    Hold { missing: u32 },
}

#[derive(Clone, Copy, Debug)]
pub struct ProgressMonitor<const TASKS: usize> {
    seen: u32,
}

impl<const TASKS: usize> ProgressMonitor<TASKS> {
    #[must_use]
    pub const fn new() -> Self {
        const { assert!(TASKS >= 1 && TASKS <= 32) };
        Self { seen: 0 }
    }

    const ALL: u32 = if TASKS == 32 {
        u32::MAX
    } else {
        (1 << TASKS) - 1
    };

    pub fn progress(&mut self, task: usize) -> bool {
        if task >= TASKS {
            return false;
        }
        self.seen |= 1 << task;
        true
    }

    pub fn check(&mut self) -> Verdict {
        let missing = Self::ALL & !self.seen;
        if missing == 0 {
            self.seen = 0;
            Verdict::Feed
        } else {
            Verdict::Hold { missing }
        }
    }
}

impl<const TASKS: usize> Default for ProgressMonitor<TASKS> {
    fn default() -> Self {
        Self::new()
    }
}
