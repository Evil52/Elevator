pub const SELF_CHECK_TIMEOUT_MS: u64 = 2_000;
pub const INPUTS_TIMEOUT_MS: u64 = 5_000;
pub const RECOVERY_HOLD_MS: u64 = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Boot,
    SelfCheck,
    InputsReady,
    Running,
    Fault,
    Recovery,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Inputs {
    pub self_check: Option<bool>,
    pub inputs_ready: bool,
    pub fault: bool,
    pub recovered: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Supervisor {
    state: State,
    since_ms: u64,
    latched: bool,
}

impl Supervisor {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: State::Boot,
            since_ms: 0,
            latched: false,
        }
    }

    #[must_use]
    pub const fn state(&self) -> State {
        self.state
    }

    #[must_use]
    pub const fn outputs_enabled(&self) -> bool {
        matches!(self.state, State::Running)
    }

    pub fn step(&mut self, now_ms: u64, inputs: Inputs) -> State {
        let elapsed = now_ms.saturating_sub(self.since_ms);
        let next = match self.state {
            State::Boot => State::SelfCheck,
            State::SelfCheck => match inputs.self_check {
                _ if inputs.fault => State::Fault,
                Some(true) => State::InputsReady,
                Some(false) => State::Fault,
                None if elapsed >= SELF_CHECK_TIMEOUT_MS => State::Fault,
                None => State::SelfCheck,
            },
            State::InputsReady => {
                if inputs.fault {
                    State::Fault
                } else if inputs.inputs_ready {
                    State::Running
                } else if elapsed >= INPUTS_TIMEOUT_MS {
                    State::Fault
                } else {
                    State::InputsReady
                }
            }
            State::Running => {
                if inputs.fault || !inputs.inputs_ready {
                    State::Fault
                } else {
                    State::Running
                }
            }
            State::Fault => {
                if inputs.recovered && !self.latched {
                    State::Recovery
                } else {
                    State::Fault
                }
            }
            State::Recovery => {
                if inputs.fault {
                    State::Fault
                } else if inputs.inputs_ready && elapsed >= RECOVERY_HOLD_MS {
                    State::InputsReady
                } else {
                    State::Recovery
                }
            }
        };
        if next != self.state {
            if self.state == State::SelfCheck && next == State::Fault {
                self.latched = true;
            }
            self.state = next;
            self.since_ms = now_ms;
        }
        next
    }
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new()
    }
}
