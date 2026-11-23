use crate::supervisor::State;

#[must_use]
pub const fn led3_on(state: State, critical_alarm: bool) -> bool {
    match state {
        State::Running => critical_alarm,
        State::Fault | State::Recovery => true,
        State::Boot | State::SelfCheck | State::InputsReady => false,
    }
}
