use elevator_ctrl::calls::Rejection as CallRejection;
use elevator_ctrl::eta::{Call, Direction as CoreDirection};
use elevator_wire::protocol::{
    CallFloor, CommandResult, CommandStatus, Direction, Envelope, Message, Rejection,
};

use crate::scenario::{PressError, Scenario};

const fn core_direction(direction: Direction) -> CoreDirection {
    match direction {
        Direction::Up => CoreDirection::Up,
        Direction::Down => CoreDirection::Down,
    }
}

const fn rejection(error: PressError) -> Rejection {
    match error {
        PressError::NotRunning | PressError::Call(CallRejection::IdExhausted) => {
            Rejection::WrongState
        }
        PressError::Call(CallRejection::NoSuchFloor) => Rejection::NoSuchFloor,
        PressError::Call(CallRejection::DownFromFirst) => Rejection::DownFromFirst,
        PressError::Call(CallRejection::UpFromTop) => Rejection::UpFromTop,
    }
}

fn call_floor(command: CallFloor, scenario: &mut Scenario, now_ms: u64) -> CommandStatus {
    let call = Call {
        floor: command.floor,
        direction: core_direction(command.direction),
    };
    match scenario.press(call, now_ms) {
        Ok(_) => CommandStatus::Accepted,
        Err(error) => CommandStatus::Rejected(rejection(error)),
    }
}

#[must_use]
pub fn execute(
    boot_id: u32,
    run_id: u32,
    envelope: &Envelope,
    scenario: &mut Scenario,
    now_ms: u64,
) -> Option<CommandResult> {
    let Message::CallFloor(command) = envelope.message else {
        return None;
    };
    let status = if envelope.boot_id != boot_id {
        CommandStatus::Rejected(Rejection::WrongBoot)
    } else if envelope.run_id != run_id {
        CommandStatus::Rejected(Rejection::WrongRun)
    } else {
        call_floor(command, scenario, now_ms)
    };
    Some(CommandResult {
        request_id: command.request_id,
        status,
    })
}
