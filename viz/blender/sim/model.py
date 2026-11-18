"""Генератор демонстрационной записи Elevator с примерной кинематикой и ETA."""
import itertools
import json
from math import isqrt
from pathlib import Path

from playback import position_at, rounded, sample

VIZ = Path(__file__).resolve().parents[2]
CONFIG = json.loads((VIZ / "config.json").read_text())
CARS = tuple("ABC")


def ceil_div(n, d):
    return -(-n // d)


def height(floor):
    return (floor - 1) * CONFIG["floorHeightMm"]


def travel_phases(distance, start_speed=0):
    if not distance:
        return []
    a, limit = CONFIG["accelMmPerSec2"], CONFIG["maxSpeedMmPerSec"]
    peak = min(limit, isqrt((2 * a * distance + start_speed**2) // 2))
    peak = max(1, peak, start_speed)
    cruise = max(0, 2 * a * distance - (2 * peak**2 - start_speed**2))
    parts = [(ceil_div((peak - start_speed) * 1000, a), start_speed, peak),
             (rounded(cruise * 1000, 2 * a * peak), peak, peak),
             (ceil_div(peak * 1000, a), peak, 0)]
    return [part for part in parts if part[0]]


def motion_from_parts(car, start, end, when, parts):
    # Распределяем миллиметры по интегралу скорости, замыкая конечную точку.
    total_area = sum(dt * (v0 + v1) for dt, v0, v1 in parts)
    area = offset = elapsed = 0
    sign = 1 if end >= start else -1
    profile = []
    for dt, v0, v1 in parts:
        area += dt * (v0 + v1)
        next_offset = rounded((end - start) * area, total_area)
        profile.append(dict(dtMs=dt, xMm=offset, dxMm=next_offset - offset,
                            v0MmPerSec=sign * v0, v1MmPerSec=sign * v1))
        offset, elapsed = next_offset, elapsed + dt
    return dict(car=car, fromMm=start, toMm=end, t0Ms=when,
                t1Ms=when + elapsed, profile=profile)


def make_motion(car, start, end, when, start_speed=0):
    return motion_from_parts(car, start, end, when,
                             travel_phases(abs(end - start), abs(start_speed)))


def speed_at(motion, t):
    return sample(motion, t)[1]


def eta(position, speed, floor, direction, wait_ms=0, target_floor=None, locked=False):
    if locked:
        return None
    distance = height(floor) - position
    if target_floor is not None:
        route = height(target_floor) - position
        if distance * route < 0 or abs(distance) > abs(route):
            return None
        if floor != target_floor and direction != ("UP" if route > 0 else "DOWN"):
            return None
    if speed and (speed * distance <= 0 or speed**2 > 2 * CONFIG["accelMmPerSec2"] * abs(distance)):
        return None
    return wait_ms + sum(dt for dt, _, _ in travel_phases(abs(distance), abs(speed)))


def assign(matrix, allowed=None, distances=None, free=None):
    # Перебор частичных инъективных назначений. Строки по id, столбцы по регистрации.
    n = len(matrix[0])
    if allowed is None:
        allowed = [[int(value is not None) for value in row] for row in matrix]
    if distances is None:
        distances = matrix
    best_key, best = None, ()
    for cols in itertools.product(range(n + 1), repeat=len(CARS)):
        pairs = [(r, c) for r, c in enumerate(cols) if c != n]
        if len({c for _, c in pairs}) != len(pairs) or any(not allowed[r][c] or matrix[r][c] is None for r, c in pairs):
            continue
        key = (-len(pairs), sum(matrix[r][c] for r, c in pairs), cols)
        if best_key is None or key < best_key:
            best_key, best = key, pairs
    greedy, available = [], set(range(len(CARS)) if free is None else free)
    for c in range(n):
        r = min((r for r in available if allowed[r][c] and matrix[r][c] is not None),
                key=lambda r: (distances[r][c], r), default=None)
        if r is not None:
            greedy.append((r, c))
            available.remove(r)

    def result(pairs):
        return {"pairs": [dict(car=CARS[r], column=c, etaMs=matrix[r][c]) for r, c in pairs],
                "totalEtaMs": sum(matrix[r][c] for r, c in pairs)}
    return result(best), result(greedy)


def dispatch(cars, calls):
    calls = sorted(calls, key=lambda c: (c["registeredMs"], c["id"]))
    matrix = [[eta(**cars[car], floor=c["floor"], direction=c["direction"])
               for c in calls] for car in CARS]
    allowed = [[int(value is not None) for value in row] for row in matrix]
    distances = [[abs(cars[car]["position"] - height(c["floor"])) for c in calls] for car in CARS]
    free = [i for i, car in enumerate(CARS) if not cars[car].get("target_floor")
            and not cars[car].get("wait_ms") and not cars[car]["speed"]]
    opt, greedy = assign(matrix, allowed, distances, free)
    return dict(columns=[c["id"] for c in calls], rows=list(CARS), matrix=matrix, allowed=allowed, optimal=opt, greedy=greedy)


def scenario():
    initial = {car: height(f) for car, f in CONFIG["initialFloors"].items()}
    calls = [dict(id="H03", floor=3, direction="DOWN", registeredMs=0),
             dict(id="H08", floor=8, direction="UP", registeredMs=0)]
    events, motions, doors = [], [], []
    line = dict(schemaVersion=1, source="python-demo", initial=initial, calls=calls, events=events, motions=motions, doors=doors)
    door_time, hold = CONFIG["doorTravelMs"], CONFIG["doorHoldMs"]
    ready = CONFIG["initialDoorHoldMs"] + door_time

    def event(t, kind, **fields):
        events.append(dict(tMs=t, type=kind, **fields))

    def transition(t, call, state, car=None, kind="CALL"):
        fields = dict(call=call["id"], floor=call["floor"], registeredMs=call["registeredMs"], state=state)
        event(t, kind, **fields, **({"car": car} if car else {}))

    def door(car, floor, start, end, left, right):
        doors.append(dict(car=car, floor=floor, t0Ms=start, t1Ms=end,
                          fromPermille=left, toPermille=right))

    def passage(car, floor, when, call=None):
        if call:
            transition(when, call, "ARRIVED", car)
        else:
            event(when, "DESTINATION", car=car, floor=floor)
        door(car, floor, when, when + door_time, 0, 1000)
        if call:
            transition(when + door_time, call, "SERVED", car)
        close = when + door_time + hold
        event(close, "DOOR_CLOSING", car=car, floor=floor)
        door(car, floor, close, close + door_time, 1000, 0)
        return close + door_time

    def travel(car, floor, start, when):
        motion = make_motion(car, start, height(floor), when)
        motions.append(motion)
        event(when, "DEPART", car=car, floor=floor)
        return motion

    def finish_trip(car, call, start, when):
        m = travel(car, call["floor"], start, when)
        done = passage(car, call["floor"], m["t1Ms"], call)
        dest = 1 if call["direction"] == "DOWN" else 10
        ride = travel(car, dest, m["toMm"], done)
        return passage(car, dest, ride["t1Ms"])

    event(0, "START")
    door("C", 10, 0, ready - door_time, 1000, 1000)
    door("C", 10, ready - door_time, ready, 1000, 0)
    event(ready - door_time, "DOOR_CLOSING", car="C", floor=10)
    for c in calls:
        transition(0, c, "QUEUED")
    cars = {car: dict(position=p, speed=0, wait_ms=ready if car == "C" else 0) for car, p in initial.items()}
    first = dispatch(cars, calls)
    event(0, "DISPATCH", **first)
    assignments = {calls[p["column"]]["id"]: p["car"] for p in first["optimal"]["pairs"]}
    for c in calls:
        transition(0, c, "ASSIGNED", assignments[c["id"]])
    healthy, failed = (assignments[c["id"]] for c in calls)
    lower, upper = calls
    lower_done = finish_trip(healthy, lower, initial[healthy], 0)
    run = travel(failed, upper["floor"], initial[failed], 0)
    # B1 приходит при достижении шестого этажа на исходной траектории.
    lo, hi = 0, run["t1Ms"]
    while lo < hi:
        mid = (lo + hi) // 2
        if sample(run, mid)[0] < height(6):
            lo = mid + 1
        else:
            hi = mid
    fault = lo
    at, velocity = sample(run, fault)
    elapsed = fault - run["t0Ms"]
    completed = []
    for phase in run["profile"]:
        if elapsed <= 0:
            break
        if elapsed < phase["dtMs"]:
            phase = dict(phase, dtMs=elapsed, v1MmPerSec=velocity,
                         dxMm=at - run["fromMm"] - phase["xMm"])
        completed.append(phase)
        elapsed -= phase["dtMs"]
    run.update(t1Ms=fault, toMm=at, profile=completed)
    event(fault, "ALARM", car=failed, code="B1", reason="LANDING_LOCK_LOST")
    stop_time = ceil_div(abs(velocity) * 1000, CONFIG["emergencyDecelMmPerSec2"])
    stop = at + rounded(velocity * stop_time, 2000)
    brake = motion_from_parts(failed, at, stop, fault, [(stop_time, abs(velocity), 0)])
    motions.append(brake)
    event(brake["t1Ms"], "LOCKED", car=failed)
    transition(fault, upper, "QUEUED", kind="REQUEUE")
    live_motion = next(m for m in motions if m["car"] == healthy)
    cars[failed]["locked"] = True
    cars[healthy] = dict(position=position_at(line, healthy, fault), speed=speed_at(live_motion, fault),
                         target_floor=lower["floor"])
    cars["C"]["wait_ms"] = max(0, ready - fault)
    second = dispatch(cars, [upper])
    event(fault, "DISPATCH", **second)
    replacement = second["optimal"]["pairs"][0]["car"]
    transition(fault, upper, "ASSIGNED", replacement)
    upper_done = finish_trip(replacement, upper, initial[replacement], max(ready, fault))
    diagnostic = max(lower_done, upper_done)
    event(diagnostic, "DIAGNOSTICS", car=failed)
    line["durationMs"] = diagnostic + CONFIG["diagnosticsMs"]
    event(line["durationMs"], "RESET")
    events.sort(key=lambda e: e["tMs"])
    motions.sort(key=lambda m: m["t0Ms"])
    doors.sort(key=lambda d: d["t0Ms"])
    return line


def serialize(line):
    return (json.dumps(line, ensure_ascii=False, indent=2, allow_nan=False) + "\n").encode()


if __name__ == "__main__":
    out = VIZ / "out" / "timeline.json"
    out.parent.mkdir(exist_ok=True)
    line = scenario()
    out.write_bytes(serialize(line))
    print(f"{out}: {len(line['events'])} событий")
