use elevator_fw_logic::commands::{Reply, answer, execute};
use elevator_fw_logic::scenario::{Scenario, StepEvents};
use elevator_wire::protocol::{
    AckAlarm, CallFloor, CommandResult, CommandStatus, Direction, Envelope, EventKind, Hello,
    InjectFault, Message, PROTOCOL_VERSION, ReadLog, RecoverCar, Rejection, ResetReason, SimAction,
    SimControl,
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

fn inject(request_id: u32, car: u8) -> Message {
    Message::InjectFault(InjectFault { request_id, car })
}

fn recover(request_id: u32, car: u8) -> Message {
    Message::RecoverCar(RecoverCar { request_id, car })
}

fn sim(request_id: u32, action: SimAction) -> Message {
    Message::SimControl(SimControl { request_id, action })
}

fn run(scenario: &mut Scenario, message: Message) -> Option<CommandResult> {
    execute(BOOT, RUN, &envelope(BOOT, RUN, message), scenario, 0)
}

#[allow(clippy::unnecessary_wraps)]
fn reply(request_id: u32, status: CommandStatus) -> Option<CommandResult> {
    Some(CommandResult { request_id, status })
}

#[test]
fn a_fault_takes_the_car_out_and_a_recovery_brings_it_back() {
    let mut scenario = running();
    assert_eq!(
        run(&mut scenario, inject(1, 0)),
        reply(1, CommandStatus::Applied)
    );
    assert!(scenario.is_failed(0));
    assert_eq!(
        run(&mut scenario, recover(2, 0)),
        reply(2, CommandStatus::Applied)
    );
    assert!(!scenario.is_failed(0));
}

#[test]
fn fault_and_recovery_name_their_rejections() {
    let mut scenario = running();
    assert_eq!(
        run(&mut scenario, inject(1, 9)),
        reply(1, CommandStatus::Rejected(Rejection::NoSuchCar))
    );
    assert_eq!(
        run(&mut scenario, recover(2, 0)),
        reply(2, CommandStatus::Rejected(Rejection::WrongState))
    );
    run(&mut scenario, inject(3, 0));
    assert_eq!(
        run(&mut scenario, inject(4, 0)),
        reply(4, CommandStatus::Rejected(Rejection::WrongState))
    );
    assert_eq!(
        run(&mut scenario, recover(5, 9)),
        reply(5, CommandStatus::Rejected(Rejection::NoSuchCar))
    );
}

#[test]
fn pause_and_resume_apply_once_each() {
    let mut scenario = running();
    assert_eq!(
        run(&mut scenario, sim(1, SimAction::Pause)),
        reply(1, CommandStatus::Applied)
    );
    assert!(scenario.is_paused());
    assert_eq!(
        run(&mut scenario, sim(2, SimAction::Pause)),
        reply(2, CommandStatus::Rejected(Rejection::WrongState))
    );
    assert_eq!(
        run(&mut scenario, sim(3, SimAction::Resume)),
        reply(3, CommandStatus::Applied)
    );
    assert_eq!(
        run(&mut scenario, sim(4, SimAction::Resume)),
        reply(4, CommandStatus::Rejected(Rejection::WrongState))
    );
}

#[test]
fn reset_restarts_the_scenario_and_clears_failures() {
    let mut scenario = running();
    run(&mut scenario, inject(1, 2));
    run(&mut scenario, sim(2, SimAction::Pause));
    assert_eq!(
        run(&mut scenario, sim(3, SimAction::Reset)),
        reply(3, CommandStatus::Applied)
    );
    assert!(!scenario.is_failed(2));
    assert!(!scenario.is_paused());
    assert_eq!(scenario.step(10), StepEvents::default());
    assert_eq!(
        run(&mut scenario, call(9, 3, Direction::Down)),
        reply(9, CommandStatus::Accepted)
    );
    assert_eq!(scenario.step(20).assigned, 1);
}

#[test]
fn every_command_checks_the_boot_and_the_run() {
    let mut scenario = running();
    let messages = [inject(1, 0), recover(2, 0), sim(3, SimAction::Pause)];
    for message in messages {
        let id = match message {
            Message::InjectFault(m) => m.request_id,
            Message::RecoverCar(m) => m.request_id,
            Message::SimControl(m) => m.request_id,
            _ => 0,
        };
        assert_eq!(
            execute(
                BOOT,
                RUN,
                &envelope(BOOT + 1, RUN, message),
                &mut scenario,
                0
            ),
            reply(id, CommandStatus::Rejected(Rejection::WrongBoot))
        );
        assert_eq!(
            execute(
                BOOT,
                RUN,
                &envelope(BOOT, RUN + 1, message),
                &mut scenario,
                0
            ),
            reply(id, CommandStatus::Rejected(Rejection::WrongRun))
        );
    }
    assert!(!scenario.is_failed(0));
    assert!(!scenario.is_paused());
}

#[test]
fn acknowledging_an_alarm_finds_no_alarm_yet() {
    let mut scenario = running();
    let ack = Message::AckAlarm(AckAlarm {
        request_id: 8,
        alarm_id: 1,
    });
    assert_eq!(
        run(&mut scenario, ack),
        reply(8, CommandStatus::Rejected(Rejection::NoSuchAlarm))
    );
}

fn ack(request_id: u32, alarm_id: u32) -> Message {
    Message::AckAlarm(AckAlarm {
        request_id,
        alarm_id,
    })
}

#[test]
fn acknowledging_a_real_alarm_is_applied_once() {
    let mut scenario = running();
    run(&mut scenario, inject(1, 0));
    assert_eq!(
        run(&mut scenario, ack(2, 1)),
        reply(2, CommandStatus::Applied)
    );
    assert_eq!(
        run(&mut scenario, ack(3, 1)),
        reply(3, CommandStatus::Rejected(Rejection::WrongState))
    );
    assert_eq!(
        run(&mut scenario, ack(4, 99)),
        reply(4, CommandStatus::Rejected(Rejection::NoSuchAlarm))
    );
}

fn ask(scenario: &mut Scenario, boot: u32, run_id: u32, message: Message) -> Option<Reply> {
    answer(BOOT, RUN, &envelope(boot, run_id, message), scenario, 0)
}

#[test]
fn reading_the_log_returns_a_chunk_from_the_requested_index() {
    let mut scenario = running();
    scenario.fail_car(0).ok();
    scenario.recover_car(0).ok();
    let read = Message::ReadLog(ReadLog {
        request_id: 5,
        from_index: 1,
    });
    let Some(Reply::Log(chunk)) = ask(&mut scenario, BOOT, RUN, read) else {
        panic!("expected a log chunk");
    };
    assert_eq!(chunk.first_index, 1);
    assert_eq!(
        chunk.events[0].map(|event| event.kind),
        Some(EventKind::AlarmCleared)
    );
    assert_eq!(chunk.events[1], None);
}

#[test]
fn reading_the_log_checks_the_boot_and_the_run() {
    let mut scenario = running();
    let read = Message::ReadLog(ReadLog {
        request_id: 6,
        from_index: 0,
    });
    assert_eq!(
        ask(&mut scenario, BOOT + 1, RUN, read),
        Some(Reply::Result(CommandResult {
            request_id: 6,
            status: CommandStatus::Rejected(Rejection::WrongBoot)
        }))
    );
    assert_eq!(
        ask(&mut scenario, BOOT, RUN + 1, read),
        Some(Reply::Result(CommandResult {
            request_id: 6,
            status: CommandStatus::Rejected(Rejection::WrongRun)
        }))
    );
}

#[test]
fn other_commands_are_answered_with_a_result() {
    let mut scenario = running();
    assert_eq!(
        ask(&mut scenario, BOOT, RUN, inject(7, 0)),
        Some(Reply::Result(CommandResult {
            request_id: 7,
            status: CommandStatus::Applied
        }))
    );
    let hello = Message::Hello(Hello {
        device_id: 1,
        reset_reason: ResetReason::PowerOn,
    });
    assert_eq!(ask(&mut scenario, BOOT, RUN, hello), None);
}
