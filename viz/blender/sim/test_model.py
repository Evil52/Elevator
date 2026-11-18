import json
import unittest
from pathlib import Path

from model import (assign, eta, make_motion, position_at, scenario, serialize,
                   speed_at)
from model import CONFIG

VIZ = Path(__file__).resolve().parents[2]


class MotionTests(unittest.TestCase):
    def test_trapezoid_and_exact_stop(self):
        motion = make_motion("A", 0, 9000, 500)
        line = {"initial": {"A": 0}, "motions": [motion]}
        for t, mm in [(0, 0), (1500, 750), (2500, 3000), (3500, 6000),
                      (4500, 8250), (5500, 9000), (9000, 9000)]:
            self.assertEqual(position_at(line, "A", t), mm)
        self.assertEqual(speed_at(motion, 1500), 1500)
        self.assertEqual(speed_at(motion, 5500), 0)

    def test_short_reverse_trip(self):
        motion = make_motion("C", 1000, 727, 0)
        line = {"initial": {"C": 1000}, "motions": [motion]}
        samples = [position_at(line, "C", t) for t in range(motion["t1Ms"] + 2)]
        self.assertTrue(all(a >= b for a, b in zip(samples, samples[1:])))
        self.assertEqual(samples[-1], 727)

    def test_golden(self):
        line = scenario()
        for vector in json.loads((VIZ / "golden.json").read_text()):
            self.assertEqual(position_at(line, vector["car"], vector["tMs"]), vector["positionMm"])


class DispatchTests(unittest.TestCase):
    def test_maximum_pairs_before_eta_and_greedy(self):
        cases = [
            ([[3, 4], [4, 90], [20, 10]], (8, 13)),
            ([[1, 4], [3, None], [None, None]], (7, 1)),
            ([[None, None], [None, 0], [None, None]], (0, 0))]
        for matrix, sums in cases:
            opt, greedy = assign(matrix)
            self.assertEqual((opt["totalEtaMs"], greedy["totalEtaMs"]), sums)
            self.assertEqual(len({p["car"] for p in opt["pairs"]}), len(opt["pairs"]))
            for p in opt["pairs"]:
                self.assertIsNotNone(matrix[ord(p["car"])-65][p["column"]])

    def test_forbidden_numeric_cell_never_wins(self):
        best, greedy = assign([[0], [50], [8]], allowed=[[0], [1], [0]])
        self.assertEqual([best["pairs"][0]["car"], greedy["pairs"][0]["car"]], ["B", "B"])

    def test_greedy_uses_distance_and_only_currently_free_cars(self):
        from model import dispatch
        call = {"id":"X", "registeredMs":0, "floor":3, "direction":"UP"}
        cars = {"A": dict(position=0, speed=0)}
        cars.update(B=dict(position=5999, speed=0, wait_ms=5000),
                    C=dict(position=3000, speed=0))
        result = dispatch(cars, [call])
        self.assertEqual(result["greedy"]["pairs"][0]["car"], "C")
        for state in cars.values():
            state["wait_ms"] = 3000
        self.assertEqual(dispatch(cars, [call])["greedy"]["pairs"], [])

    def test_ties_are_stable(self):
        opt, _ = assign([[1, 1], [1, 1], [1, 1]])
        self.assertEqual([(p["car"], p["column"]) for p in opt["pairs"]], [("A", 0), ("B", 1)])

    def test_collective_routes_and_door_residual(self):
        self.assertEqual(eta(0, 0, 3, "DOWN", wait_ms=700), 4700)
        # Кабина едет вверх к восьмому этажу.
        self.assertIsNotNone(eta(3000, 1500, 5, "UP", target_floor=8))
        self.assertIsNone(eta(3000, 1500, 5, "DOWN", target_floor=8))
        self.assertIsNone(eta(3000, 1500, 2, "UP", target_floor=8))
        self.assertIsNone(eta(2990, 3000, 2, "UP", target_floor=8))
        self.assertIsNotNone(eta(3000, 1500, 8, "DOWN", target_floor=8))
        self.assertIsNone(eta(0, 0, 3, "DOWN", locked=True))
        self.assertLess(eta(3000, 1500, 6, "UP", target_floor=8), eta(3000, 0, 6, "UP"))

    def test_chronological_call_order(self):
        from model import dispatch
        calls = [{"id":"Y", "floor":3, "direction":"DOWN","registeredMs":4},
                 {"id":"Z", "floor":3, "direction":"DOWN","registeredMs":1},
                 {"id":"A", "floor":3, "direction":"DOWN","registeredMs":4}]
        cars = {name: dict(position=0, speed=0) for name in "ABC"}
        self.assertEqual(dispatch(cars, calls)["columns"], ["Z", "A", "Y"])


class ScenarioTests(unittest.TestCase):
    def test_demo_timeline_identifies_its_generator(self):
        line = scenario()
        self.assertEqual(line["source"], "python-demo")

    def test_failure_requeues_original_call_and_opens_before_served(self):
        line = scenario()
        events = line["events"]
        self.assertEqual(events, sorted(events, key=lambda e: e["tMs"]))
        matrices = [e for e in events if e["type"] == "DISPATCH"]
        self.assertLess(matrices[0]["optimal"]["totalEtaMs"], matrices[0]["greedy"]["totalEtaMs"])
        alarm = next(e for e in events if e["type"] == "ALARM")
        self.assertTrue(any(m["car"] == alarm["car"] and m["t0Ms"] == alarm["tMs"]
                            and m["fromMm"] != m["toMm"] for m in line["motions"]))
        affected = next(e["call"] for e in events if e["type"] == "REQUEUE")
        transitions = [e for e in events if e.get("call") == affected and "state" in e]
        self.assertEqual([e["state"] for e in transitions],
                         ["QUEUED", "ASSIGNED", "QUEUED", "ASSIGNED", "ARRIVED", "SERVED"])
        self.assertEqual(len({e["registeredMs"] for e in transitions}), 1)
        self.assertNotEqual(transitions[1]["car"], transitions[3]["car"])
        for e in events:
            if e.get("state") == "SERVED":
                self.assertTrue(any(d["car"] == e["car"] and d["floor"] == e["floor"]
                                    and d["t1Ms"] == e["tMs"] and d["toPermille"] == 1000 for d in line["doors"]))
        self.assertEqual([e["type"] for e in events[-2:]], ["DIAGNOSTICS", "RESET"])

    def test_interrupted_profile_ends_at_emergency_brake(self):
        line = scenario()
        for move in line["motions"]:
            self.assertEqual(sum(p["dtMs"] for p in move["profile"]), move["t1Ms"] - move["t0Ms"])
            self.assertEqual(sum(p["dxMm"] for p in move["profile"]), move["toMm"] - move["fromMm"])
        before, brake = [m for m in line["motions"] if m["car"] == "A"]
        self.assertEqual(before["profile"][-1]["v1MmPerSec"], brake["profile"][0]["v0MmPerSec"])

    def test_determinism_and_integer_data(self):
        raw = serialize(scenario())
        self.assertEqual(raw, serialize(scenario()))
        json.loads(raw, parse_float=lambda value: self.fail(value))
        self.assertTrue(CONFIG["approximate"])
