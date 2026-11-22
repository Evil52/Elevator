use elevator_fw_logic::commands::execute;
use elevator_fw_logic::scenario::{Scenario, StepEvents};
use elevator_wire::protocol::{
    CallFloor, CommandResult, CommandStatus, Direction, Envelope, Hello, Message, PROTOCOL_VERSION,
    Rejection, ResetReason,
};

const BOOT: u32 = 77;
const RUN: u32 = 3;

fn envelope(boot_id: u32, run_id: u32, message: Message) -> Envelope {
    Envelope {
        version: PROTOCOL_VERSION,
        boot_id,
        run_id,
        seq: 0,
        message,
    }
}

fn call(request_id: u32, floor: u8, direction: Direction) -> Message {
    Message::CallFloor(CallFloor {
        request_id,
        floor,
        direction,
    })
}

fn running() -> Scenario {
    let mut scenario = Scenario::new();
    scenario.start_empty();
    scenario
}

#[test]
fn a_valid_call_is_accepted_and_reaches_the_dispatcher() {
    let mut scenario = running();
    let reply = execute(
        BOOT,
        RUN,
        &envelope(BOOT, RUN, call(9, 3, Direction::Down)),
        &mut scenario,
        0,
    );
    assert_eq!(
        reply,
        Some(CommandResult {
            request_id: 9,
            status: CommandStatus::Accepted
        })
    );
    assert_eq!(scenario.step(10).assigned, 1);
}

#[test]
fn the_same_call_sent_twice_makes_one_call() {
    let mut scenario = running();
    let first = envelope(BOOT, RUN, call(1, 8, Direction::Up));
    let again = envelope(BOOT, RUN, call(2, 8, Direction::Up));
    assert_eq!(
        execute(BOOT, RUN, &first, &mut scenario, 0),
        Some(CommandResult {
            request_id: 1,
            status: CommandStatus::Accepted
        })
    );
    assert_eq!(
        execute(BOOT, RUN, &again, &mut scenario, 5),
        Some(CommandResult {
            request_id: 2,
            status: CommandStatus::Accepted
        })
    );
    assert_eq!(scenario.step(10).assigned, 1);
}

#[test]
fn a_command_for_another_boot_is_rejected_and_changes_nothing() {
    let mut scenario = running();
    let stale = envelope(BOOT + 1, RUN, call(4, 3, Direction::Down));
    assert_eq!(
        execute(BOOT, RUN, &stale, &mut scenario, 0),
        Some(CommandResult {
            request_id: 4,
            status: CommandStatus::Rejected(Rejection::WrongBoot)
        })
    );
    assert_eq!(scenario.step(10), StepEvents::default());
}

#[test]
fn a_command_for_an_old_run_is_rejected_and_changes_nothing() {
    let mut scenario = running();
    let stale = envelope(BOOT, RUN - 1, call(5, 3, Direction::Down));
    assert_eq!(
        execute(BOOT, RUN, &stale, &mut scenario, 0),
        Some(CommandResult {
            request_id: 5,
            status: CommandStatus::Rejected(Rejection::WrongRun)
        })
    );
    assert_eq!(scenario.step(10), StepEvents::default());
}

#[test]
fn the_boot_is_checked_before_the_run() {
    let mut scenario = running();
    let both = envelope(BOOT + 1, RUN + 1, call(6, 3, Direction::Down));
    assert_eq!(
        execute(BOOT, RUN, &both, &mut scenario, 0),
        Some(CommandResult {
            request_id: 6,
            status: CommandStatus::Rejected(Rejection::WrongBoot)
        })
    );
}

#[test]
fn impossible_buttons_are_rejected_with_their_reason() {
    let mut scenario = running();
    let cases = [
        (0, Direction::Up, Rejection::NoSuchFloor),
        (11, Direction::Down, Rejection::NoSuchFloor),
        (1, Direction::Down, Rejection::DownFromFirst),
        (10, Direction::Up, Rejection::UpFromTop),
    ];
    for (id, (floor, direction, reason)) in (10_u32..).zip(cases) {
        assert_eq!(
            execute(
                BOOT,
                RUN,
                &envelope(BOOT, RUN, call(id, floor, direction)),
                &mut scenario,
                0
            ),
            Some(CommandResult {
                request_id: id,
                status: CommandStatus::Rejected(reason)
            })
        );
    }
    assert_eq!(scenario.step(10), StepEvents::default());
}

#[test]
fn a_call_before_the_scenario_runs_is_a_wrong_state() {
    let mut scenario = Scenario::new();
    assert_eq!(
        execute(
            BOOT,
            RUN,
            &envelope(BOOT, RUN, call(7, 3, Direction::Down)),
            &mut scenario,
            0
        ),
        Some(CommandResult {
            request_id: 7,
            status: CommandStatus::Rejected(Rejection::WrongState)
        })
    );
}

#[test]
fn messages_that_are_not_commands_get_no_reply() {
    let mut scenario = running();
    let hello = envelope(
        BOOT,
        RUN,
        Message::Hello(Hello {
            device_id: 1,
            reset_reason: ResetReason::PowerOn,
        }),
    );
    assert_eq!(execute(BOOT, RUN, &hello, &mut scenario, 0), None);
}
