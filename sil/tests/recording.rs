use std::{fs, path::Path, process::Command};

#[test]
fn both_exports_match_the_checked_in_viewer_recordings() {
    for (args, scenario) in [(vec![], "single-trip"), (vec!["--scenario", "b1"], "b1")] {
        let output = Command::new(env!("CARGO_BIN_EXE_elevator-sil"))
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success());
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../viz/recordings")
            .join(scenario)
            .join("timeline.json");
        assert_eq!(output.stdout, fs::read(path).unwrap(), "{scenario}");
    }
}

#[test]
fn dispatch_reassigns_a_call_and_ignores_late_car_events() {
    use elevator_ctrl::{
        assign,
        calls::{CallTable, TransitionError},
        eta::{
            Building, Call, CarMode, CarState, Direction, DoorTimes, EtaParams, QueuedCall,
            build_matrix,
        },
        motion::MotionParams,
    };
    let params = EtaParams {
        building: Building::new(10, 3000).unwrap(),
        motion: MotionParams::new(3000, 1500, 3000).unwrap(),
        doors: DoorTimes::new(2400, 5000, 2400).unwrap(),
        aging_cap_ms: 0,
    };
    let a = CarState {
        position_mm: 6000,
        speed_mm_s: 0,
        accel_mm_s2: 0,
        heading: Direction::Up,
        door_remaining_ms: 0,
        up_calls: 0,
        down_calls: 0,
        car_calls: 0,
        mode: CarMode::Normal,
    };
    let mut cars = [
        a,
        CarState {
            position_mm: 0,
            ..a
        },
    ];
    let call = Call {
        floor: 4,
        direction: Direction::Up,
    };
    let mut table = CallTable::<10>::new();
    table.press(call, 125).unwrap();
    let dispatch = |table: &mut CallTable<10>, cars: &[CarState; 2], waited_ms| {
        let mut queue = [call; 1];
        assert_eq!(table.queued(&mut queue), 1);
        let calls = queue.map(|call| QueuedCall { call, waited_ms });
        let matched = assign(&build_matrix(&params, cars, &calls));
        let row = matched.iter().position(Option::is_some).unwrap();
        table.assign(queue[0], u8::try_from(row).unwrap()).unwrap()
    };

    let first = dispatch(&mut table, &cars, 0);
    assert_eq!(first.car, 0);
    cars[0].mode = CarMode::Unavailable;
    assert_eq!(table.car_lost(first.car), 1);
    let next = dispatch(&mut table, &cars, 1000);
    assert_eq!((next.car, next.id), (1, first.id));
    assert_eq!(table.arrived(first), Err(TransitionError::StaleAssignment));
    table.arrived(next).unwrap();
    assert_eq!(
        table.doors_opened(first),
        Err(TransitionError::StaleAssignment)
    );
    let completed = table.doors_opened(next).unwrap();
    assert_eq!(
        (completed.id, completed.car, completed.registered_ms),
        (first.id, 1, 125)
    );
    assert_eq!(table.get(call), None);
}
