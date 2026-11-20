use std::io::{self, Write};

use elevator_ctrl::{
    Cell, assign,
    calls::{CallRecord, CallState, CallTable},
    eta::{
        Building, Call, CarMode, CarState, Direction, DoorTimes, EtaParams, QueuedCall,
        build_matrix,
    },
    motion::{MotionParams, RestTrip},
};
use serde_json::{Value, json};

const CAR_IDS: [&str; 3] = ["A", "B", "C"];
const FLOOR_HEIGHT_MM: u32 = 3000;
const OPEN_MS: u32 = 2400;
const SAMPLE_MS: u32 = 50;

fn profile(trip: &RestTrip) -> Vec<Value> {
    let mut phases = Vec::new();
    let mut start = 0;
    let mut previous = trip.sample(start);
    let end = trip.duration_ms();
    for t in (SAMPLE_MS..end).step_by(SAMPLE_MS as usize).chain([end]) {
        let next = trip.sample(t);
        phases.push(json!({
            "dtMs":t-start, "xMm":previous.0, "dxMm":next.0-previous.0,
            "v0MmPerSec":previous.1, "v1MmPerSec":next.1
        }));
        previous = next;
        start = t;
    }
    phases
}

fn assignment(matrix: &[[Cell; 1]; 3], car: usize) -> Value {
    let eta = matrix[car][0].eta_ms;
    json!({"pairs":[{"car":CAR_IDS[car],"column":0,"etaMs":eta}],"totalEtaMs":eta})
}

fn call_event(time: u32, floor: u8, state: &str, registered_ms: u64, car: Option<u8>) -> Value {
    let mut event = json!({"type":"CALL","tMs":time,"call":"H04","floor":floor,"registeredMs":registered_ms,"state":state});
    if let Some(car) = car {
        event["car"] = json!(CAR_IDS[usize::from(car)]);
    }
    event
}

fn active_call_event(time: u32, floor: u8, record: CallRecord) -> Value {
    let state = match record.state {
        CallState::Queued => "QUEUED",
        CallState::Assigned => "ASSIGNED",
        CallState::Arrived => "ARRIVED",
    };
    call_event(time, floor, state, record.registered_ms, record.car)
}

fn trip_recording() -> Value {
    let params = EtaParams {
        building: Building::new(10, FLOOR_HEIGHT_MM).unwrap(),
        motion: MotionParams::new(3000, 1500, 3000).unwrap(),
        doors: DoorTimes::new(OPEN_MS, 5000, 2400).unwrap(),
        aging_cap_ms: 0,
    };
    let a = CarState {
        position_mm: 0,
        speed_mm_s: 0,
        accel_mm_s2: 0,
        heading: Direction::Up,
        door_remaining_ms: 0,
        up_calls: 0,
        down_calls: 0,
        car_calls: 0,
        mode: CarMode::Normal,
    };
    let cars = [
        a,
        CarState {
            position_mm: 6 * FLOOR_HEIGHT_MM,
            mode: CarMode::Unavailable,
            ..a
        },
        CarState {
            position_mm: 9 * FLOOR_HEIGHT_MM,
            mode: CarMode::Unavailable,
            ..a
        },
    ];
    let call = Call {
        floor: 4,
        direction: Direction::Up,
    };
    let floor = call.floor;
    let target_mm = u32::from(floor - 1) * FLOOR_HEIGHT_MM;
    let mut table = CallTable::<10>::new();
    table.press(call, 0).unwrap();
    let queued = active_call_event(0, floor, table.get(call).unwrap());
    let matrix = build_matrix(&params, &cars, &[QueuedCall { call, waited_ms: 0 }]);
    let chosen = assign(&matrix).iter().position(Option::is_some).unwrap();
    let greedy = (0..cars.len())
        .filter(|&i| matrix[i][0].allowed)
        .min_by_key(|&i| cars[i].position_mm.abs_diff(target_mm))
        .unwrap();
    let car = CAR_IDS[chosen];
    let start_mm = cars[chosen].position_mm;
    let trip = params.motion.rest_trip(target_mm - start_mm).unwrap();
    let arrival = trip.duration_ms();
    assert_eq!(
        u64::from(arrival),
        matrix[chosen][0].eta_ms,
        "ETA и профиль перегона разошлись"
    );
    let served = arrival + OPEN_MS;
    let half_ramp = trip.ramp_ms() / 2;
    let ticket = table.assign(call, u8::try_from(chosen).unwrap()).unwrap();
    let assigned = active_call_event(0, floor, table.get(call).unwrap());
    table.arrived(ticket).unwrap();
    let arrived = active_call_event(arrival, floor, table.get(call).unwrap());
    let completed = table.doors_opened(ticket).unwrap();
    let doors_open = call_event(
        served,
        floor,
        "SERVED",
        completed.registered_ms,
        Some(completed.car),
    );
    json!({
        "schemaVersion":1, "source":"rust-sil","durationMs":served,
        "initial":{"A":cars[0].position_mm,"B":cars[1].position_mm,"C":cars[2].position_mm},
        "calls":[{"id":"H04","floor":floor,"direction":"UP","registeredMs":0}],
        "events":[
            {"tMs":0,"type":"START"}, queued,
            {"type":"DISPATCH","tMs":0,"rows":CAR_IDS,"columns":["H04"],
             "matrix":matrix.map(|[c]| [c.allowed.then_some(c.eta_ms)]),
             "allowed":matrix.map(|[c]| [u8::from(c.allowed)]),
             "optimal":assignment(&matrix,chosen),"greedy":assignment(&matrix,greedy)},
            assigned, {"type":"DEPART","tMs":0,"car":car,"floor":floor},
            {"type":"SAMPLE_ACCEL","tMs":half_ramp,"car":car},
            {"type":"SAMPLE_CRUISE","tMs":arrival/2,"car":car},
            {"type":"SAMPLE_BRAKE","tMs":arrival-half_ramp,"car":car},
            arrived, doors_open, {"type":"END","tMs":served}
        ],
        "motions":[{"car":car,"t0Ms":0,"t1Ms":arrival,"fromMm":start_mm,"toMm":target_mm,"profile":profile(&trip)}],
        "doors":[{"car":car,"floor":floor,"t0Ms":arrival,"t1Ms":served,"fromPermille":0,"toPermille":1000}]
    })
}

fn main() -> io::Result<()> {
    let mut out = io::BufWriter::new(io::stdout().lock());
    serde_json::to_writer_pretty(&mut out, &trip_recording())?;
    writeln!(out)?;
    out.flush()
}
