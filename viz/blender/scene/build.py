"""Собирает разрез и запекает события модели в анимацию Blender и GLB."""
import argparse
import json
import sys
from pathlib import Path

import bpy
from mathutils import Vector
from bpy_extras.anim_utils import action_get_channelbag_for_slot

SCENE_DIR = Path(__file__).resolve().parent
VIZ = SCENE_DIR.parents[1]
sys.path.insert(0, str(SCENE_DIR))
sys.path.insert(0, str(VIZ / 'blender/sim'))

from check import times_for_car, validate
from geometry import DOOR_TRAVEL, architecture, color


def key(obj, channel, t):
    obj.keyframe_insert(data_path=channel, frame=t, group=obj.name)


def animate(timeline, parts):
    cars, doors, landings, lamps = parts
    if timeline['motions']:
        from model import position_at
    else:
        def position_at(data, car, t):
            return data['initial'][car]

    scene = bpy.context.scene
    # Кадр равен миллисекунде. Экспортер переносит шкалу в секунды glTF.
    scene.render.fps = 1000
    scene.frame_start, scene.frame_end = 0, timeline['durationMs']
    for car, obj in cars.items():
        for t in times_for_car(timeline, car):
            obj.location.z = position_at(timeline, car, t) / 1000
            key(obj, 'location', t)
        lamp = lamps[car]
        lamp.scale = (0.00001,) * 3
        key(lamp, 'scale', 0)
        for event in timeline['events']:
            is_alarm = event['type'] == 'ALARM' and event.get('car') == car
            if is_alarm or event['type'] == 'RESET':
                t = event['tMs']
                key(lamp, 'scale', max(0, t-1))
                lamp.scale = (1.0 if is_alarm else .00001,) * 3
                key(lamp, 'scale', t)
    for car in cars:
        for side, sign in [('L', -1), ('R', 1)]:
            panels = [(doors[(car, side)], None)]
            panels += [(landings[(car, n, side)], n) for n in range(1, 11)]
            for obj, floor in panels:
                cycles = [d for d in timeline['doors'] if d['car'] == car and floor in (None, d['floor'])]
                if not cycles:
                    continue
                origin = obj.location.x
                key(obj, 'location', 0)
                for cycle in cycles:
                    for when, amount in [('t0Ms', 'fromPermille'), ('t1Ms', 'toPermille')]:
                        obj.location.x = origin + sign * DOOR_TRAVEL * cycle[amount] / 1000
                        key(obj, 'location', cycle[when])
                for event in timeline['events']:
                    if event['type'] == 'RESET' and cycles[-1]['t1Ms'] <= event['tMs']:
                        key(obj, 'location', max(cycles[-1]['t1Ms'], event['tMs']-1))
                        initial = cycles[0]['fromPermille'] if cycles[0]['t0Ms'] == 0 else 0
                        obj.location.x = origin + sign * DOOR_TRAVEL * initial / 1000
                        key(obj, 'location', event['tMs'])
    # Скриптовая вставка ключей использует Bézier независимо от настроек интерфейса Blender.
    for obj in scene.objects:
        data = obj.animation_data
        if data and data.action:
            bag = action_get_channelbag_for_slot(data.action, data.action_slot)
            for curve in bag.fcurves:
                for point in curve.keyframe_points:
                    point.interpolation = 'LINEAR'
                curve.update()
    scene.frame_set(0)


def point(obj, target):
    obj.rotation_euler = (Vector(target) - obj.location).to_track_quat('-Z', 'Y').to_euler()


def studio():
    scene = bpy.context.scene
    scene.render.engine = 'CYCLES'
    scene.cycles.device = 'CPU'
    scene.cycles.samples = 12
    scene.cycles.use_denoising = True
    scene.render.resolution_x, scene.render.resolution_y = 740, 840
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = 'PNG'
    scene.view_settings.view_transform = 'AgX'
    world = bpy.data.worlds.new('Studio')
    scene.world = world
    world.use_nodes = True
    shader = world.node_tree.nodes.get('Background')
    shader.inputs['Color'].default_value = color('97a8c0')
    shader.inputs['Strength'].default_value = .55
    # Раздельные лучи дают ровный фон и сохраняют мягкие отражения металла.
    nodes = world.node_tree.nodes
    light_path = nodes.new('ShaderNodeLightPath')
    mix = nodes.new('ShaderNodeMixShader')
    background = nodes.new('ShaderNodeBackground')
    background.inputs[0].default_value = color('18222b')
    background.inputs[1].default_value = 1
    links = world.node_tree.links
    links.new(light_path.outputs['Is Camera Ray'], mix.inputs[0])
    links.new(shader.outputs[0], mix.inputs[1])
    links.new(background.outputs[0], mix.inputs[2])
    links.new(mix.outputs[0], nodes.get('World Output').inputs[0])
    bpy.ops.object.camera_add(location=(21, -56, 30))
    camera = bpy.context.object
    camera.data.type, camera.data.ortho_scale = 'ORTHO', 35.4
    point(camera, (0,0,15))
    scene.camera = camera
    for loc, energy, size, tint in [((-14,-18,35),10000,18,'fff2d8'),
                                    ((12,-9,17),6500,14,'dbe8ff'),
                                    ((-5,12,27),12000,11,'dae9ff')]:
        bpy.ops.object.light_add(type='AREA', location=loc)
        lamp = bpy.context.object
        lamp.data.energy, lamp.data.shape, lamp.data.size = energy, 'DISK', size
        lamp.data.color = color(tint)[:3]
        point(lamp, (0,0,15))


def run():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--timeline', type=Path, default=VIZ / 'out/timeline.json')
    parser.add_argument('--out', type=Path, default=VIZ / 'out')
    parser.add_argument('--skip-previews', action='store_true')
    args = parser.parse_args(sys.argv[sys.argv.index('--')+1:] if '--' in sys.argv else [])
    args.out.mkdir(parents=True, exist_ok=True)
    timeline = json.loads(args.timeline.read_text())
    bpy.ops.wm.read_factory_settings(use_empty=True)
    animate(timeline, architecture())
    path = args.out / 'scene.glb'
    bpy.ops.export_scene.gltf(filepath=str(path), export_format='GLB', export_force_sampling=False,
                              export_animation_mode='ACTIVE_ACTIONS', export_optimize_animation_size=False,
                              export_optimize_animation_keep_anim_object=True, export_yup=True,
                              export_nla_strips_merged_animation_name='Scenario', export_cameras=False,
                              export_lights=False)
    validate(timeline, path)
    if args.skip_previews:
        return
    studio()
    events = timeline['events']
    def first(predicate):
        return next((e['tMs'] for e in events if predicate(e)), 0)
    moments = [('dispatch', 0), ('alarm', first(lambda e: e['type']=='ALARM')),
               ('served', first(lambda e: e.get('state')=='SERVED'))]
    for name, t in moments:
        scene = bpy.context.scene
        scene.frame_set(t)
        scene.render.filepath = str(args.out / f'preview-{name}.png')
        bpy.ops.render.render(write_still=True)
        print(f'Превью {name} · {t} мс')


if __name__ == '__main__':
    run()
