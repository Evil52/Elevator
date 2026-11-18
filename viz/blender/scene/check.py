"""Проверяет готовый GLB независимо от Blender, включая экспортированные ключи."""
import argparse
import json
import struct
from pathlib import Path

VIZ = Path(__file__).resolve().parents[2]


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
    total = 0
    for car in 'ABC':
        expected = times_for_car(timeline, car)
        channels = [(anim, ch) for anim in doc.get('animations', []) for ch in anim['channels']
                    if nodes[ch['target']['node']] == f'Car_{car}' and ch['target']['path'] == 'translation']
        assert len(channels) == 1, f'Кабина {car}: отсутствует отдельная дорожка движения'
        anim, ch = channels[0]
        acc = doc['accessors'][anim['samplers'][ch['sampler']]['input']]
        view = doc['bufferViews'][acc['bufferView']]
        offset = view.get('byteOffset', 0) + acc.get('byteOffset', 0)
        actual = [round(t * 1000) for t in struct.unpack_from(f"<{acc['count']}f", binary, offset)]
        assert actual == expected, f'{car}: ключи GLB {actual} и модели {expected} различаются'
        total += len(actual)
    print(f'GLB OK · 3 кабины · 10 этажей · {total} ключей положения · {len(nodes)} узлов')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--timeline', type=Path, default=VIZ / 'out/timeline.json')
    parser.add_argument('--glb', type=Path, default=VIZ / 'out/scene.glb')
    args = parser.parse_args()
    validate(json.loads(args.timeline.read_text()), args.glb)
