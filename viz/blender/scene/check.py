"""Проверяет готовый GLB независимо от Blender, включая экспортированные ключи."""
import argparse
import json
import struct
import sys
from pathlib import Path

VIZ = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(VIZ / 'blender/sim'))
from playback import position_at


def read_glb(path):
    raw = path.read_bytes()
    magic, version, size = struct.unpack_from('<4sII', raw)
    assert (magic, version, size) == (b'glTF', 2, len(raw)), 'Некорректный GLB'
    length, tag = struct.unpack_from('<I4s', raw, 12)
    assert tag == b'JSON'
    return json.loads(raw[20:20 + length]), raw[28 + length:]


def times_for_car(timeline, car):
    times = {0, timeline['durationMs']}
    for event in timeline['events']:
        if event['type'] == 'RESET':
            times.update((max(0, event['tMs'] - 1), event['tMs']))
    for motion in timeline['motions']:
        if motion['car'] == car:
            times.update((motion['t0Ms'], motion['t1Ms']))
            t = motion['t0Ms']
            for phase in motion['profile']:
                t += phase['dtMs']
                times.add(t)
            times.update(range(motion['t0Ms'], motion['t1Ms'], 125))
    return sorted(times)


def validate(timeline, path):
    doc, binary = read_glb(path)
    nodes = [node['name'] for node in doc['nodes']]
    assert sorted(n for n in nodes if n.startswith('Car_')) == ['Car_A', 'Car_B', 'Car_C']
    assert sorted(n for n in nodes if n.startswith('Level_')) == [f'Level_{i:02}' for i in range(1, 11)]
    for car in 'ABC':
        assert f'Lamp_{car}' in nodes
        for side in 'LR':
            assert f'Door_{car}_{side}' in nodes
            for floor in range(1, 11):
                assert f'Landing_{car}_{floor:02}_{side}' in nodes
    def floats(index):
        acc = doc['accessors'][index]
        view = doc['bufferViews'][acc['bufferView']]
        offset = view.get('byteOffset', 0) + acc.get('byteOffset', 0)
        components = {'SCALAR': 1, 'VEC3': 3}[acc['type']]
        assert acc['componentType'] == 5126 and 'byteStride' not in view
        return struct.unpack_from(f"<{acc['count'] * components}f", binary, offset)

    def track(name, channel):
        matches = [anim['samplers'][ch['sampler']] for anim in doc.get('animations', [])
                   for ch in anim['channels'] if nodes[ch['target']['node']] == name
                   and ch['target']['path'] == channel]
        assert len(matches) == 1, f'{name}: отсутствует отдельная дорожка {channel}'
        sampler = matches[0]
        times = [round(t * 1000) for t in floats(sampler['input'])]
        assert sampler['interpolation'] == 'LINEAR', f'{name}: нелинейная интерполяция ключей'
        coords = floats(sampler['output'])
        return dict(zip(times, zip(coords[::3], coords[1::3], coords[2::3]), strict=True))

    total = 0
    for car in 'ABC':
        expected = times_for_car(timeline, car)
        positions = track(f'Car_{car}', 'translation')
        assert list(positions) == expected, f'{car}: число или время ключей отличается от модели'
        for t, (_, y, _) in positions.items():
            assert round(y * 1000) == position_at(timeline, car, t), f'{car}: высота в {t} мс'
        total += len(positions)
    for event in timeline['events']:
        t = event['tMs']
        if event['type'] == 'ALARM':
            assert track(f"Lamp_{event['car']}", 'scale')[t][0] == 1, 'Отсутствует индикация аварии'
        if event['type'] == 'RESET':
            for car in sorted({d['car'] for d in timeline['doors']}):
                panels = [f'Door_{car}_L'] + [f'Landing_{car}_{floor:02}_L'
                          for floor in sorted({d['floor'] for d in timeline['doors'] if d['car'] == car})]
                for name in panels:
                    positions = track(name, 'translation')
                    assert abs(positions[t][0] - positions[0][0]) < .0001, f'{name}: сброс положения'
    print(f'GLB OK · 3 кабины · 10 этажей · {total} ключей положения · {len(nodes)} узлов')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--timeline', type=Path, default=VIZ / 'out/timeline.json')
    parser.add_argument('--glb', type=Path, default=VIZ / 'out/scene.glb')
    args = parser.parse_args()
    validate(json.loads(args.timeline.read_text()), args.glb)
