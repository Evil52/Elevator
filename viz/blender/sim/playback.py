"""Чтение профиля движения timeline v1 в целых миллиметрах."""


def rounded(n, d):
    """Ближайшее целое, половины округляются от нуля."""
    return (1 if n >= 0 else -1) * ((2 * abs(n) + d) // (2 * d))


def sample(motion, t):
    if t >= motion["t1Ms"]:
        return motion["toMm"], 0
    elapsed = max(0, t - motion["t0Ms"])
    for p in motion["profile"]:
        dt = p["dtMs"]
        if elapsed <= dt:
            v0, v1 = abs(p["v0MmPerSec"]), abs(p["v1MmPerSec"])
            numerator = 2 * v0 * dt * elapsed + (v1 - v0) * elapsed**2
            offset = rounded(p["dxMm"] * numerator, (v0 + v1) * dt**2)
            speed = p["v0MmPerSec"] + rounded((p["v1MmPerSec"] - p["v0MmPerSec"]) * elapsed, dt)
            return motion["fromMm"] + p["xMm"] + offset, speed
        elapsed -= dt
    return motion["toMm"], 0


def position_at(line, car, t):
    if "durationMs" in line and t >= line["durationMs"] and line["events"][-1]["type"] == "RESET":
        return line["initial"][car]
    for motion in reversed(line["motions"]):
        if motion["car"] == car and motion["t0Ms"] <= t:
            return sample(motion, t)[0]
    return line["initial"][car]
