use elevator_ctrl::calls::Rejection as CallRejection;
use elevator_ctrl::eta::{Call, Direction as CoreDirection};
use elevator_wire::protocol::{
    AckAlarm, CallFloor, CommandResult, CommandStatus, Direction, Envelope, EventLogChunk,
    InjectFault, Message, RecoverCar, Rejection, SimAction, SimControl,
};

use crate::alarms::AlarmError;
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
        SimAction::FireRecall => scenario.fire_recall().is_ok(),
        SimAction::ClearFire => scenario.clear_fire().is_ok(),
    };
    applied(changed.then_some(()).ok_or(Rejection::WrongState))
}

fn ack_alarm(command: AckAlarm, scenario: &mut Scenario) -> CommandStatus {
    match scenario.acknowledge_alarm(command.alarm_id) {
        Ok(()) => CommandStatus::Applied,
        Err(AlarmError::NoSuchAlarm) => CommandStatus::Rejected(Rejection::NoSuchAlarm),
        Err(AlarmError::WrongState | AlarmError::TableFull) => {
            CommandStatus::Rejected(Rejection::WrongState)
        }
    }
}

fn identity_status(boot_id: u32, run_id: u32, envelope: &Envelope) -> Option<CommandStatus> {
    if envelope.boot_id != boot_id {
        Some(CommandStatus::Rejected(Rejection::WrongBoot))
    } else if envelope.run_id != run_id {
        Some(CommandStatus::Rejected(Rejection::WrongRun))
    } else {
        None
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
    let request_id = match envelope.message {
        Message::CallFloor(command) => command.request_id,
        Message::InjectFault(command) => command.request_id,
        Message::RecoverCar(command) => command.request_id,
        Message::SimControl(command) => command.request_id,
        Message::AckAlarm(command) => command.request_id,
        _ => return None,
    };
    let status =
        identity_status(boot_id, run_id, envelope).unwrap_or_else(|| match envelope.message {
            Message::CallFloor(command) => call_floor(command, scenario, now_ms),
            Message::InjectFault(command) => inject_fault(command, scenario),
            Message::RecoverCar(command) => recover_car(command, scenario),
            Message::SimControl(command) => sim_control(command, scenario),
            Message::AckAlarm(command) => ack_alarm(command, scenario),
            _ => CommandStatus::Unknown,
        });
    Some(CommandResult { request_id, status })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reply {
    Result(CommandResult),
    Log(EventLogChunk),
}

#[must_use]
pub fn answer(
    boot_id: u32,
    run_id: u32,
    envelope: &Envelope,
    scenario: &mut Scenario,
    now_ms: u64,
) -> Option<Reply> {
    if let Message::ReadLog(read) = envelope.message {
        return Some(match identity_status(boot_id, run_id, envelope) {
            Some(status) => Reply::Result(CommandResult {
                request_id: read.request_id,
                status,
            }),
            None => Reply::Log(scenario.events_chunk(read.from_index)),
        });
    }
    execute(boot_id, run_id, envelope, scenario, now_ms).map(Reply::Result)
}
