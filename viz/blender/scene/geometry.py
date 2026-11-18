"""Геометрия разреза. Один метр Blender соответствует одному метру glTF."""
from math import cos, pi, sin
import bpy
from mathutils import Vector

SHAFTS = {'A': -4.4, 'B': 0.0, 'C': 4.4}
COLORS = {'A': 'd4fc70', 'B': '79a8ff', 'C': 'ffac75'}
DOOR_TRAVEL = 1.05


def color(hex_value):
    def linear(n):
        n = int(n, 16) / 255
        return n / 12.92 if n < .04045 else ((n + .055) / 1.055) ** 2.4
    return tuple(linear(hex_value[i:i + 2]) for i in (0, 2, 4)) + (1,)


def material(name, tint, metal=0.0, emission=0.0):
    m = bpy.data.materials.new(name)
    m.diffuse_color = color(tint)
    m.use_nodes = True
    shader = m.node_tree.nodes.get('Principled BSDF')
    shader.inputs['Base Color'].default_value = m.diffuse_color
    shader.inputs['Metallic'].default_value = metal
    shader.inputs['Roughness'].default_value = .36 if metal else .53
    shader.inputs['Emission Color'].default_value = m.diffuse_color
    shader.inputs['Emission Strength'].default_value = emission
    return m


class Parts:
    """Общий меш для деталей с одинаковым преобразованием."""
    def __init__(self, name):
        self.name = name
        self.vertices = []
        self.faces = []
        self.materials = []
        self.indices = []

    def add(self, vertices, faces, mat):
        offset = len(self.vertices)
        if mat not in self.materials:
            self.materials.append(mat)
        self.vertices.extend(vertices)
        self.faces.extend(tuple(offset + i for i in f) for f in faces)
        self.indices.extend([self.materials.index(mat)] * len(faces))

    def box(self, x, y, z, w, d, h, mat):
        vertices = [(x + a*w/2, y + b*d/2, z + c*h/2)
                    for a, b, c in [(-1,-1,-1),(1,-1,-1),(1,1,-1),(-1,1,-1),
                                    (-1,-1,1),(1,-1,1),(1,1,1),(-1,1,1)]]
        self.add(vertices, [(3,2,1,0),(4,5,6,7),(0,1,5,4),(1,2,6,5),(2,3,7,6),(3,0,4,7)], mat)

    def rod(self, start, end, radius, mat, sides=8):
        a, b = Vector(start), Vector(end)
        axis = (b - a).normalized()
        u = axis.cross(Vector((0,1,0)) if abs(axis.y) < .8 else Vector((0,0,1))).normalized()
        v = axis.cross(u)
        ring = [radius * (u*cos(2*pi*i/sides) + v*sin(2*pi*i/sides)) for i in range(sides)]
        vertices = [p + r for p in (a, b) for r in ring]
        faces = [tuple(reversed(range(sides))), tuple(range(sides, 2*sides))]
        faces += [(i, (i+1)%sides, sides+(i+1)%sides, sides+i) for i in range(sides)]
        self.add(vertices, faces, mat)

    def finish(self, parent=None):
        mesh = bpy.data.meshes.new(self.name)
        mesh.from_pydata(self.vertices, [], self.faces)
        for mat in self.materials:
            mesh.materials.append(mat)
        mesh.polygons.foreach_set('material_index', self.indices)
        mesh.update()
        obj = bpy.data.objects.new(self.name, mesh)
        bpy.context.collection.objects.link(obj)
        if parent:
            obj.parent = parent
        return obj


def label(name, value, x, y, z, size, mat, parent=None):
    font = bpy.data.curves.new(name, 'FONT')
    font.body, font.size = value, size
    font.align_x, font.align_y = 'CENTER', 'CENTER'
    font.resolution_u = 3
    font.materials.append(mat)
    obj = bpy.data.objects.new(name, font)
    bpy.context.collection.objects.link(obj)
    obj.location = (x, y, z)
    obj.rotation_euler.x = pi / 2
    if parent:
        obj.parent = parent
    deps = bpy.context.evaluated_depsgraph_get()
    mesh = bpy.data.meshes.new_from_object(obj.evaluated_get(deps), depsgraph=deps)
    result = bpy.data.objects.new(name + '_mesh', mesh)
    bpy.context.collection.objects.link(result)
    result.parent, result.location, result.rotation_euler = parent, obj.location, obj.rotation_euler
    bpy.data.objects.remove(obj, do_unlink=True)
    result.name = name
    return result


def architecture():
    steel = material('Structural silver', 'bbc7c9', .42)
    concrete = material('Warm cast concrete', 'd0cdbe')
    frame = material('Powder coated midnight', '394655', .35)
    deep = material('Lift well graphite', '27323e', .12)
    face = material('Machined aluminium', '8796a2', .40)
    white = material('Etched lettering', 'f0f4e7', .1, .1)
    orange = material('Alarm red', 'ff383c', .06, .9)
    black = material('Window ribbon', '18222b', .25)
    colors = {key: material(f'Cab {key} enamel', tint, .16, .15) for key, tint in COLORS.items()}
    lamps = {key: material(f'Cab {key} pilot', tint, .05, .65) for key, tint in COLORS.items()}
    structure = Parts('Structure')

    # Отдельные уровни подиума и открытые кромки плиты читаются в общем виде.
    structure.box(0, .1, -.70, 16.4, 5.3, .52, frame)
    structure.box(0, .1, -.37, 16.0, 5.0, .24, concrete)
    structure.box(0, .1, -.15, 15.3, 4.55, .24, face)
    for x in (-6.62, -2.2, 2.2, 6.62):
        structure.box(x, 1.65, 14.9, .22, .28, 30.2, steel)
        structure.box(x, -1.32, 14.9, .20, .23, 30.2, face)
        structure.box(x-.075, -1.46, 14.9, .05, .055, 30.2, steel)
    for car, x in SHAFTS.items():
        structure.box(x, 1.83, 14.9, 4.26, .21, 30, deep)
        for dx in (-1.65, 1.65):
            structure.box(x+dx, 1.55, 15, .14, .15, 30, face)
        for dx in (-.32, -.18):
            structure.rod((x+dx,.82,0), (x+dx,.82,30.65), .027, face)
        # Оголовки шахт, шкив и привод на крыше.
        structure.box(x, .3, 30.20, 3.95, 3.04, .24, concrete)
        structure.box(x, .4, 30.45, 2.95, 1.64, .21, frame)
        structure.box(x-.62, .64, 30.78, .99, .8, .69, face)
        for dx, width, mat in [(0,.94,frame),(0,.69,colors[car]),(0,.43,face),(0,.15,deep)]:
            depth = 1.03 - width*.32
            structure.rod((x+.47+dx, .55, 30.9), (x+.47+dx, .55-depth, 30.9), width*.58, mat, 24)
        for i in range(5):
            structure.box(x-1.00+i*.17, .60, 31.15, .05, .81, .15, steel)
    for side in (-1, 1):
        x = side*6.68
        for i in range(10):
            z = i*3
            structure.rod((x, -1.2,z+.13), (x, 1.54,z+2.86), .048, face)
    structure.finish()

    landings = {}
    for n in range(1, 11):
        z = (n-1)*3
        level = Parts(f'Level_{n:02}')
        # Снята передняя стена: открыты направляющие и кабины, посадочные двери видны с обратной стороны.
        level.box(0, 1.38, z, 13.67, 1.08, .19, concrete)
        level.box(0, -1.33, z-.035, 13.7, .27, .14, steel)
        for x in (-6.68,-2.2,2.2,6.68):
            level.box(x, .16, z-.025, .22, 3.24, .16, steel)
        level.box(-7.14, -1.34, z+.36, .92, .16, .62, frame)
        for car, x in SHAFTS.items():
            level.box(x, 1.43, z+2.46, 2.22, .22, .14, face)
            for sign, side in [(-1, 'L'), (1, 'R')]:
                level.box(x+sign*1.12, 1.46, z+1.21, .095, .2, 2.48, face)
                p = Parts(f'Landing_{car}_{n:02}_{side}')
                p.box(0, 0, 1.17, 1.03, .10, 2.30, frame)
                obj = p.finish()
                obj.location = (x+sign*.526, 1.53, z)
                landings[(car, n, side)] = obj
        level.finish()
        label(f'Etch_{n:02}', f'{n:02}', -7.14, -1.431, z+.36, .46, white)
    cars, doors, alarm = {}, {}, {}
    for car, x in SHAFTS.items():
        cab = Parts(f'Car_{car}')
        m = colors[car]
        cab.box(0, -.12, 1.33, 3.47, 2.03, 2.48, m)
        cab.box(0, -.12, .16, 3.62, 2.18, .19, frame)
        cab.box(0, -.12, 2.64, 3.59, 2.15, .13, steel)
        cab.box(0, -1.165, 1.38, 2.28, .10, 2.15, black)
        cab.box(0, -1.222, 2.47, 2.44, .12, .13, black)
        cab.box(0, -1.291, 2.47, 1.83, .026, .063, lamps[car])
        cab.box(-1.45, -1.158, 1.84, .43, .10, .54, frame)
        cab.box(1.46, -1.162, 1.26, .21, .106, .4, face)
        cab.box(1.46,-1.224,1.35,.10,.018,.08,lamps[car])
        for dx in (-1.56,1.56):
            cab.box(dx, -.02, 2.75, .13, 1.83, .18, black)
        obj = cab.finish()
        obj.location.x = x
        cars[car] = obj
        label(f'Insignia_{car}', car, -1.45, -1.22, 1.84, .4, white, obj)
        for side, sign in [('L',-1), ('R',1)]:
            door = Parts(f'Door_{car}_{side}')
            door.box(0, 0, 1.35, 1.02, .08, 2.0, m)
            door.box(0, -.045, 1.72, .75, .019, .68, deep)
            door.box(0, -.057, 1.44, .74, .008, .047, steel)
            door.box(sign*.41, -.047, 1.07, .026, .014, .61, frame)
            door_obj = door.finish(obj)
            door_obj.location = (sign*.523, -1.25, 0)
            doors[(car,side)] = door_obj
        light = Parts(f'Lamp_{car}')
        light.box(0,0,0,1.88,.036,.09,orange)
        lamp = light.finish(obj)
        lamp.location = (0,-1.32,2.47)
        alarm[car] = lamp
    return cars, doors, landings, alarm
