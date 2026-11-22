use elevator_ctrl::calls::Rejection as CallRejection;
use elevator_ctrl::eta::{Call, Direction as CoreDirection};
use elevator_wire::protocol::{
    CallFloor, CommandResult, CommandStatus, Direction, Envelope, InjectFault, Message, RecoverCar,
    Rejection, SimAction, SimControl,
};

use crate::scenario::{CarError, PressError, Scenario};

const fn core_direction(direction: Direction) -> CoreDirection {
    match direction {
        Direction::Up => CoreDirection::Up,
        Direction::Down => CoreDirection::Down,
    }
}

const fn press_rejection(error: PressError) -> Rejection {
    match error {
        PressError::NotRunning | PressError::Call(CallRejection::IdExhausted) => {
            Rejection::WrongState
        }
        PressError::Call(CallRejection::NoSuchFloor) => Rejection::NoSuchFloor,
        PressError::Call(CallRejection::DownFromFirst) => Rejection::DownFromFirst,
        PressError::Call(CallRejection::UpFromTop) => Rejection::UpFromTop,
    }
}

const fn car_rejection(error: CarError) -> Rejection {
    match error {
        CarError::NoSuchCar => Rejection::NoSuchCar,
        CarError::WrongState => Rejection::WrongState,
    }
}

const fn applied(outcome: Result<(), Rejection>) -> CommandStatus {
    match outcome {
        Ok(()) => CommandStatus::Applied,
        Err(reason) => CommandStatus::Rejected(reason),
    }
}

fn call_floor(command: CallFloor, scenario: &mut Scenario, now_ms: u64) -> CommandStatus {
    let call = Call {
        floor: command.floor,
        direction: core_direction(command.direction),
    };
    match scenario.press(call, now_ms) {
        Ok(_) => CommandStatus::Accepted,
        Err(error) => CommandStatus::Rejected(press_rejection(error)),
    }
}

fn inject_fault(command: InjectFault, scenario: &mut Scenario) -> CommandStatus {
    applied(
        scenario
            .fail_car(usize::from(command.car))
            .map_err(car_rejection),
    )
}

fn recover_car(command: RecoverCar, scenario: &mut Scenario) -> CommandStatus {
    applied(
        scenario
            .recover_car(usize::from(command.car))
            .map_err(car_rejection),
    )
}

fn sim_control(command: SimControl, scenario: &mut Scenario) -> CommandStatus {
    let changed = match command.action {
        SimAction::Pause => scenario.set_paused(true),
        SimAction::Resume => scenario.set_paused(false),
        SimAction::Reset => scenario.reset().is_some(),
    };
    applied(changed.then_some(()).ok_or(Rejection::WrongState))
}

#[must_use]
pub fn execute(
    boot_id: u32,
    run_id: u32,
    envelope: &Envelope,
    scenario: &mut Scenario,
    now_ms: u64,
) -> Option<CommandResult> {
    let request_id = match envelope.message {
        Message::CallFloor(command) => command.request_id,
        Message::InjectFault(command) => command.request_id,
        Message::RecoverCar(command) => command.request_id,
        Message::SimControl(command) => command.request_id,
        Message::AckAlarm(command) => command.request_id,
        _ => return None,
    };
    let status = if envelope.boot_id != boot_id {
        CommandStatus::Rejected(Rejection::WrongBoot)
    } else if envelope.run_id != run_id {
        CommandStatus::Rejected(Rejection::WrongRun)
    } else {
        match envelope.message {
            Message::CallFloor(command) => call_floor(command, scenario, now_ms),
            Message::InjectFault(command) => inject_fault(command, scenario),
            Message::RecoverCar(command) => recover_car(command, scenario),
            Message::SimControl(command) => sim_control(command, scenario),
            _ => CommandStatus::Rejected(Rejection::NoSuchAlarm),
        }
    };
    Some(CommandResult { request_id, status })
}
