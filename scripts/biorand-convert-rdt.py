#!/usr/bin/env python3
"""Convert BioRand's RE1 staging map into arklay's map files.

A one-off converter. It reads `rdt.json`, `doors.csv` and `enemy.csv` from
`meta/re1` in a biorand-classic checkout (branch `staging`) and writes:

- `games/re1/biorand/map.xml`: the item catalogue, the enemy groups and the
  begin and end rooms;
- `games/re1/room/<dir>/random.xml` for every room directory, holding the
  logical rooms in that directory, then the directory's enemy spots.

The schema is documented in `crates/arklay-biorand/src/map.rs`. The files
are edited by hand after this runs once (`docs/biorand/plan.md`), so this
script only checks and explains the conversion. Run it from the repository
root:

    python3 -I scripts/biorand-convert-rdt.py <path to meta/re1>

How staging's data becomes arklay's:

- A `doors.csv` row sets the entrance of the door with that id in the one
  logical room that contains the row's RDT and has such a door. It replaces
  an inline `entrance`, as staging's GetMap does.
- An `enemy.csv` row is a spot of the room directory that holds its RDT. Every
  logical room that lists the RDT must be in that one directory, which is
  checked. Each logical room in a directory shares the directory's spots.
- A cut comes from the proof-of-concept table below, or from the inline
  entrance, but only when that recorded the same coordinates as the final
  entrance. Staging never writes cuts.
- Staging's enemy `condition`, `typeOffsets`, `name` and `ignoreRatio` are
  kept as comments. `cutscenes`, `characters` and the legacy `nop` are
  dropped. Any other field raises an error.

The output is deterministic, so running the converter again changes nothing.
"""

import csv
import json
import os
import re
import sys
import xml.etree.ElementTree as ElementTree
from collections import Counter
from types import SimpleNamespace

# The proof of concept's entrances: (room dir, door id) -> (x, y, z, d, cut).
# Its random.xml files are replaced by this script, so the cut values are
# kept here. An entry is used only where the converted entrance has the same
# coordinates; the others are written as comments.
PROOF_OF_CONCEPT = {
    ('111', 0): (2800, 0, 4600, 0, 1),
    ('111', 1): (22500, 0, 11300, 2048, 7),
    ('112', 0): (2900, 0, 15300, 0, 7),
    ('112', 1): (3500, 0, 7500, 0, 7),
    ('118', 0): (6000, 0, 8000, 1024, 1),
    ('202', 0): (6400, 0, 14000, 1024, 4),
    ('202', 1): (30900, 0, 8300, 2048, 1),
    ('204', 0): (3000, 0, 3600, 0, 1),
    ('204', 1): (18600, 0, 11300, 0, 5),
    ('204', 2): (3500, 0, 19100, 512, 4),
    ('204', 3): (13000, 0, 19200, 1024, 7),
    ('204', 4): (12500, 0, 2800, 3072, 7),
    ('207', 0): (3000, 0, 3600, 3072, 7),
    ('207', 1): (17000, 0, 11300, 3072, 5),
    ('207', 2): (30400, 0, 5000, 3072, 4),
    ('209', 0): (9100, 0, 7700, 2048, 9),
    ('213', 0): (15800, 0, 16500, 0, 4),
    ('213', 2): (9800, 0, 2900, 0, 1),
    ('213', 3): (2500, 0, 6600, 0, 1),
    ('216', 0): (13600, 0, 10300, 2048, 4),
    ('216', 1): (3000, 0, 3600, 0, 5),
    ('219', 0): (7400, 0, 2900, 2048, 1),
    ('21a', 1): (2500, 0, 3500, 0, 7),
    ('21b', 0): (27500, 0, 12600, 2048, 7),
    ('21b', 1): (4800, 0, 4600, 0, 0),
    ('303', 8): (29130, 0, 5700, 2048, 4),
}

# Staging targets that name a door the room doesn't have, keyed by (room, door
# id): (staging target, corrected target). Staging names the RDT's door 1, which
# is on the sub-room that has it. Any other mismatch is an error.
TARGET_FIXUPS = {
    ('306', 0): ('30F:1', '30F/BACK:1'),
    ('40E', 0): ('40D:1', '40D/EXIT:1'),
}

# Fields of each staging object. Dropped fields are read and discarded; the
# rest must be handled, so a field this table doesn't know is an error.
TOP_FIELDS = {'beginEndRooms', 'items', 'enemies', 'rooms'}
DROPPED_TOP_FIELDS = {'characters'}  # actor sheets, not used by the randomizer
BEGIN_END_FIELDS = {'start', 'end'}
CATALOGUE_FIELDS = {'name', 'kind', 'max', 'ammo', 'group', 'amount', 'discard'}
GROUP_FIELDS = {'name', 'background', 'foreground', 'entries'}
ENTRY_FIELDS = {'key', 'name', 'id', 'esp', 'poses'}
POSE_FIELDS = {'name', 'pose', 'probability'}
ROOM_FIELDS = {'name', 'rdts', 'tags', 'doors', 'items', 'enemies', 'flags'}
DROPPED_ROOM_FIELDS = {'cutscenes'}  # cosmetic actor data
DOOR_FIELDS = {
    'id', 'name', 'target', 'entrance', 'kind', 'lockId', 'lockKey',
    'requires2', 'allowedLocks', 'noUnlock', 'randomize', 'tags', 'doorRando',
    'player',
}
ENTRANCE_FIELDS = {'x', 'y', 'z', 'd', 'cut'}
ITEM_FIELDS = {
    'name', 'id', 'globalId', 'type', 'amount', 'group', 'player', 'requires2',
    'lockId', 'optional', 'document', 'hidden', 'doorRando',
}
COMMENTED_ITEM_FIELDS = {'typeOffsets'}  # hard-coded item types in the scripts
ENEMY_FIELDS = {
    'id', 'kind', 'globalId', 'includeTypes', 'excludeTypes', 'maxDifficulty',
    'restricted', 'player',
}
COMMENTED_ENEMY_FIELDS = {'condition', 'name', 'ignoreRatio'}
DROPPED_ENEMY_FIELDS = {'nop'}  # a legacy rewrite of script bytes
FLAG_FIELDS = {'name', 'requires2'}

DOOR_KINDS = {'dynamic', 'blocked', 'unblock', 'locked', 'unlock', 'oneway', 'noreturn'}
PLAYERS = {0: 'chris', 1: 'jill'}

ROOM_KEY = re.compile(r'[0-9A-F]{3}(/[A-Z0-9]+)?')
TARGET = re.compile(r'[0-9A-F]{3}(/[A-Z0-9]+)?(:[0-9]+)?')
RDT_ID = re.compile(r'[0-9A-F]{3}')
REQUIREMENT = re.compile(r'(item|flag|room)\(([A-Za-z0-9/_]+)\)')
ENEMY_KEY = re.compile(r'[a-z0-9_]+')

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ROOM_DIR = os.path.join(ROOT, 'games', 're1', 'room')
MAP_FILE = os.path.join(ROOT, 'games', 're1', 'biorand', 'map.xml')

I32 = (-2**31, 2**31 - 1)


class ConvertError(Exception):
    pass


def fail(where, message):
    raise ConvertError(f'{where}: {message}')


# JSON with comments and trailing commas (staging's rdt.json is JSONC).

def _string_end(text, start):
    """Index just past the string literal that starts at text[start]."""
    i = start + 1
    while i < len(text) and text[i] != '"':
        i += 2 if text[i] == '\\' else 1
    if i >= len(text):
        raise ConvertError('unterminated string literal')
    return i + 1


def strip_jsonc(text):
    out = []
    i = 0
    while i < len(text):
        if text[i] == '"':
            end = _string_end(text, i)
            out.append(text[i:end])
            i = end
        elif text.startswith('//', i):
            newline = text.find('\n', i)
            i = len(text) if newline < 0 else newline
        elif text.startswith('/*', i):
            close = text.find('*/', i + 2)
            if close < 0:
                raise ConvertError('unterminated /* comment')
            i = close + 2
        else:
            out.append(text[i])
            i += 1
    text = ''.join(out)
    out = []
    i = 0
    while i < len(text):
        if text[i] == '"':
            end = _string_end(text, i)
            out.append(text[i:end])
            i = end
        elif text[i] == ',':
            j = i + 1
            while j < len(text) and text[j].isspace():
                j += 1
            if j < len(text) and text[j] in ']}':
                i += 1  # a trailing comma
            else:
                out.append(',')
                i += 1
        else:
            out.append(text[i])
            i += 1
    return ''.join(out)


def read_json(path):
    with open(path, encoding='utf-8') as f:
        text = f.read()

    def no_duplicate_keys(pairs):
        keys = [key for key, _ in pairs]
        duplicates = sorted({key for key in keys if keys.count(key) > 1})
        if duplicates:
            raise ConvertError(f'{path}: duplicate keys {duplicates}')
        return dict(pairs)

    def reject(name):
        raise ConvertError(f'{path}: {name} is not allowed')

    return json.loads(
        strip_jsonc(text),
        object_pairs_hook=no_duplicate_keys,
        parse_constant=reject,
    )


# Field access.

def check_fields(obj, where, *allowed):
    if not isinstance(obj, dict):
        fail(where, 'expected an object')
    unknown = sorted(set(obj) - set().union(*allowed))
    if unknown:
        fail(where, 'unknown field(s): ' + ', '.join(unknown))


def count_dropped(obj, fields, dropped, label):
    for field in sorted(fields):
        if field in obj:
            dropped[f'{label} {field}'] += 1


def int_field(obj, key, where, lo, hi, required=False):
    if key not in obj:
        if required:
            fail(where, f'{key} is missing')
        return None
    value = obj[key]
    if type(value) is not int or not lo <= value <= hi:
        fail(where, f'{key} must be an integer from {lo} to {hi}, not {value!r}')
    return value


def str_field(obj, key, where, required=False):
    if key not in obj:
        if required:
            fail(where, f'{key} is missing')
        return None
    value = obj[key]
    if not isinstance(value, str) or value == '':
        fail(where, f'{key} must be a non-empty string, not {value!r}')
    return value


def bool_field(obj, key, where):
    if key not in obj:
        return None
    value = obj[key]
    if type(value) is not bool:
        fail(where, f'{key} must be true or false, not {value!r}')
    return value


def list_field(obj, key, where, item):
    """A JSON list of items, checked by `item(value, where)`; None if absent."""
    if key not in obj:
        return None
    value = obj[key]
    if not isinstance(value, list):
        fail(where, f'{key} must be a list')
    return [item(v, f'{where}.{key}') for v in value]


def int_list_field(obj, key, where, lo, hi):
    def item(value, where):
        if type(value) is not int or not lo <= value <= hi:
            fail(where, f'expected an integer from {lo} to {hi}, not {value!r}')
        return value
    return list_field(obj, key, where, item)


def str_list_field(obj, key, where):
    def item(value, where):
        if not isinstance(value, str) or value == '':
            fail(where, f'expected a non-empty string, not {value!r}')
        return value
    return list_field(obj, key, where, item)


def requirement_list_field(obj, key, where):
    def item(value, where):
        match = REQUIREMENT.fullmatch(value) if isinstance(value, str) else None
        if match is None or (match.group(1) == 'item' and not match.group(2).isdigit()):
            fail(where, f'{value!r} is not item(N), flag(NAME) or room(KEY)')
        return (match.group(1), match.group(2))
    return list_field(obj, key, where, item)


def player_field(obj, where):
    value = int_field(obj, 'player', where, 0, 1)
    return None if value is None else PLAYERS[value]


# Staging's data, as tables the rest of the converter uses.

def read_catalogue(staging):
    catalogue = {}
    for key, obj in staging['items'].items():
        where = f'rdt.json items.{key}'
        if not key.isdigit():
            fail(where, 'item ids are numbers')
        check_fields(obj, where, CATALOGUE_FIELDS)
        catalogue[int(key)] = SimpleNamespace(
            id=int(key),
            name=str_field(obj, 'name', where, required=True),
            kind=str_field(obj, 'kind', where, required=True),
            max=int_field(obj, 'max', where, 0, 65535),
            ammo=int_list_field(obj, 'ammo', where, 0, 255),
            group=int_field(obj, 'group', where, *I32),
            amount=int_field(obj, 'amount', where, 0, 255),
            discard=bool_field(obj, 'discard', where),
        )
    return catalogue


def read_enemy_groups(staging):
    groups = []
    for gi, obj in enumerate(staging['enemies']):
        where = f'rdt.json enemies[{gi}]'
        check_fields(obj, where, GROUP_FIELDS)
        entries = []
        for ei, entry in enumerate(obj['entries']):
            ewhere = f'{where}.entries[{ei}]'
            check_fields(entry, ewhere, ENTRY_FIELDS)
            key = str_field(entry, 'key', ewhere, required=True)
            if not ENEMY_KEY.fullmatch(key):
                fail(ewhere, f'enemy key {key!r} is not a configuration key')
            poses = []
            for pi, pose in enumerate(entry.get('poses') or []):
                pwhere = f'{ewhere}.poses[{pi}]'
                check_fields(pose, pwhere, POSE_FIELDS)
                probability = pose.get('probability')
                if type(probability) not in (int, float) or not 0 <= probability <= 1:
                    fail(pwhere, f'probability must be from 0 to 1, not {probability!r}')
                poses.append(SimpleNamespace(
                    name=str_field(pose, 'name', pwhere, required=True),
                    pose=int_field(pose, 'pose', pwhere, 0, 255, required=True),
                    probability=float(probability),
                ))
            entries.append(SimpleNamespace(
                key=key,
                name=str_field(entry, 'name', ewhere, required=True),
                ids=int_list_field(entry, 'id', ewhere, 0, 255) or [],
                esp=int_list_field(entry, 'esp', ewhere, 0, 255) or [],
                poses=poses,
            ))
        groups.append(SimpleNamespace(
            name=str_field(obj, 'name', where, required=True),
            background=str_field(obj, 'background', where, required=True),
            foreground=str_field(obj, 'foreground', where, required=True),
            entries=entries,
        ))
    return groups


def read_entrance(obj, where):
    if obj is None:
        return None
    check_fields(obj, where, ENTRANCE_FIELDS)
    return SimpleNamespace(
        x=int_field(obj, 'x', where, *I32, required=True),
        y=int_field(obj, 'y', where, *I32, required=True),
        z=int_field(obj, 'z', where, *I32, required=True),
        d=int_field(obj, 'd', where, *I32, required=True),
        cut=int_field(obj, 'cut', where, 0, 255),
    )


def read_door(room, index, obj, catalogue):
    where = f'{room.key} doors[{index}]'
    check_fields(obj, where, DOOR_FIELDS)
    kind = str_field(obj, 'kind', where)
    if kind is not None and kind not in DOOR_KINDS:
        fail(where, f'unknown door kind {kind!r}')
    key = int_field(obj, 'lockKey', where, 0, 255)
    if key is not None and key not in catalogue:
        fail(where, f'lockKey {key} is not in the item catalogue')
    lock = int_field(obj, 'lockId', where, 1, 255)
    door = SimpleNamespace(
        room=room.key,
        id=int_field(obj, 'id', where, 0, 255),
        name=str_field(obj, 'name', where),
        target=str_field(obj, 'target', where),
        kind=kind,
        lock=lock,
        key=key,
        allowed=int_list_field(obj, 'allowedLocks', where, 0, 255),
        no_unlock=bool_field(obj, 'noUnlock', where),
        randomize=bool_field(obj, 'randomize', where),
        requires=requirement_list_field(obj, 'requires2', where) or [],
        tags=str_list_field(obj, 'tags', where) or [],
        door_rando=bool_field(obj, 'doorRando', where),
        player=player_field(obj, where),
        inline=read_entrance(obj.get('entrance'), f'{where}.entrance'),
        comments=[],
        entrance_comments=[],
        entrance=None,
        cut=None,
    )
    if key is not None and lock is None:
        door.comments.append(
            f'staging lockKey {key} has no lockId, so staging leaves this door unlocked'
        )
    return door


def read_item(room, index, obj):
    where = f'{room.key} items[{index}]'
    check_fields(obj, where, ITEM_FIELDS, COMMENTED_ITEM_FIELDS)
    return SimpleNamespace(
        id=int_field(obj, 'id', where, 0, 255, required=True),
        flag=int_field(obj, 'globalId', where, 0, 65535),
        kind=int_field(obj, 'type', where, 0, 255),
        amount=int_field(obj, 'amount', where, 0, 255),
        group=int_field(obj, 'group', where, *I32),
        name=str_field(obj, 'name', where),
        requires=requirement_list_field(obj, 'requires2', where) or [],
        lock=int_field(obj, 'lockId', where, 1, 255),
        optional=bool_field(obj, 'optional', where),
        document=bool_field(obj, 'document', where),
        hidden=bool_field(obj, 'hidden', where),
        door_rando=bool_field(obj, 'doorRando', where),
        player=player_field(obj, where),
        type_offsets=str_list_field(obj, 'typeOffsets', where) or [],
    )


def read_rule(room, index, obj):
    where = f'{room.key} enemies[{index}]'
    check_fields(obj, where, ENEMY_FIELDS, COMMENTED_ENEMY_FIELDS, DROPPED_ENEMY_FIELDS)
    return SimpleNamespace(
        id=int_field(obj, 'id', where, 0, 255),
        flag=int_field(obj, 'globalId', where, 0, 65535),
        kind=str_field(obj, 'kind', where),
        include=int_list_field(obj, 'includeTypes', where, 0, 255),
        exclude=int_list_field(obj, 'excludeTypes', where, 0, 255),
        max_difficulty=int_field(obj, 'maxDifficulty', where, 0, 255),
        restricted=bool_field(obj, 'restricted', where),
        player=player_field(obj, where),
        condition=str_field(obj, 'condition', where),
        name=str_field(obj, 'name', where),
        ignore_ratio=bool_field(obj, 'ignoreRatio', where),
    )


def read_rooms(staging, catalogue, dropped):
    rooms = {}
    for key, obj in staging['rooms'].items():
        where = f'rdt.json rooms.{key}'
        if not ROOM_KEY.fullmatch(key):
            fail(where, 'room keys are RRR or RRR/PART')
        check_fields(obj, where, ROOM_FIELDS, DROPPED_ROOM_FIELDS)
        count_dropped(obj, DROPPED_ROOM_FIELDS, dropped, 'room')
        rdts = str_list_field(obj, 'rdts', where) or []
        for rdt in rdts:
            if not RDT_ID.fullmatch(rdt):
                fail(where, f'{rdt!r} is not an RDT id')
        room = SimpleNamespace(
            key=key,
            name=str_field(obj, 'name', where, required=True),
            tags=str_list_field(obj, 'tags', where) or [],
            rdts=rdts,
            doors=[],
            flags=[],
            items=[],
            enemies=[],
        )
        room.doors = [
            read_door(room, i, d, catalogue)
            for i, d in enumerate(obj.get('doors') or [])
        ]
        room.items = [read_item(room, i, item) for i, item in enumerate(obj.get('items') or [])]
        for e in obj.get('enemies') or []:
            count_dropped(e, DROPPED_ENEMY_FIELDS, dropped, 'enemy rule')
        room.enemies = [read_rule(room, i, e) for i, e in enumerate(obj.get('enemies') or [])]
        for i, flag in enumerate(obj.get('flags') or []):
            where = f'{key} flags[{i}]'
            check_fields(flag, where, FLAG_FIELDS)
            room.flags.append(SimpleNamespace(
                name=str_field(flag, 'name', where, required=True),
                requires=requirement_list_field(flag, 'requires2', where) or [],
            ))
        rooms[key] = room
    return rooms


def read_csv_rows(path, header, trailing_optional=0):
    """Rows of a CSV with `header`. The last `trailing_optional` columns may be
    left out of a row, which then reads them as empty (as staging's CSV reader
    does with its header names)."""
    with open(path, encoding='utf-8', newline='') as f:
        rows = list(csv.reader(f))
    if not rows or rows[0] != header:
        fail(path, f'expected the header {",".join(header)}')
    fewest = len(header) - trailing_optional
    result = []
    for n, row in enumerate(rows[1:], start=2):
        if not fewest <= len(row) <= len(header):
            fail(f'{path}:{n}', f'expected {fewest} to {len(header)} columns')
        result.append(row + [''] * (len(header) - len(row)))
    return result


def csv_int(value, where):
    try:
        return int(value)
    except ValueError:
        fail(where, f'{value!r} is not an integer')


# Assigning doors, entrances and spots.

def coords(entrance):
    return (entrance.x, entrance.y, entrance.z, entrance.d)


def describe(entrance):
    return f'x={entrance.x} y={entrance.y} z={entrance.z} d={entrance.d} cut={entrance.cut}'


def key_base(key):
    return key.split('/')[0]


def target_resolves(rooms, target):
    room_key, _, door_id = target.partition(':')
    if room_key not in rooms:
        return False
    if door_id == '':
        return True
    return any(d.id == int(door_id) for d in rooms[room_key].doors)


def resolve_targets(rooms, report):
    for room in rooms.values():
        for door in room.doors:
            if door.target is None:
                continue
            if not TARGET.fullmatch(door.target):
                fail(f'{room.key} door {door.id}', f'target {door.target!r} is not RRR or RRR:ID')
            if target_resolves(rooms, door.target):
                continue
            fixup = TARGET_FIXUPS.get((room.key, door.id))
            if fixup is None or fixup[0] != door.target or not target_resolves(rooms, fixup[1]):
                fail(f'{room.key} door {door.id}', f'target {door.target} names no door')
            door_id = int(door.target.split(':')[1])
            new_room = fixup[1].split(':')[0]
            door.comments.append(
                f'staging target {door.target} has no door {door_id} in {door.target.split(":")[0]}; '
                f'door {door_id} is in {new_room}'
            )
            door.target = fixup[1]
            report['targets fixed'] += 1


def dir_of_rdt(rooms, rdt):
    # The room directory that holds an RDT: every logical room listing it is in it.
    dirs = {key_base(r.key).lower() for r in rooms.values() if rdt in r.rdts}
    if len(dirs) != 1:
        fail('enemy.csv', f'RDT {rdt} is in {len(dirs)} room directories, not one')
    return dirs.pop()


def assign_doors(rooms, door_rows, report):
    seen = {}
    for n, row in enumerate(door_rows, start=2):
        where = f'doors.csv:{n}'
        target, *values = row
        if not re.fullmatch(r'[0-9A-F]{3}:[0-9]+', target):
            fail(where, f'{target!r} is not RRR:ID')
        x, y, z, d = (csv_int(v, where) for v in values)
        rdt, door_id = target.split(':')
        door_id = int(door_id)
        if target in seen:
            if seen[target] != (x, y, z, d):
                fail(where, f'{target} is listed twice with different entrances')
            report['doors.csv duplicate rows'] += 1
            duplicated = True
        else:
            seen[target] = (x, y, z, d)
            duplicated = False
        candidates = [
            r for r in rooms.values()
            if rdt in r.rdts and any(d.id == door_id for d in r.doors)
        ]
        if len(candidates) != 1:
            fail(where, f'{target} matches {len(candidates)} logical rooms, not one')
        room = candidates[0]
        for door in room.doors:
            if door.id != door_id:
                continue
            entrance = SimpleNamespace(x=x, y=y, z=z, d=d, cut=None)
            if door.inline is not None and coords(door.inline) != (x, y, z, d):
                door.entrance_comments.append(
                    f'staging entrance {describe(door.inline)} replaced by doors.csv'
                )
            if duplicated:
                door.entrance_comments.append(
                    f'doors.csv lists {target} twice with the same entrance'
                )
            else:
                report['doors from doors.csv'] += 1
            door.entrance = entrance


def assign_inline_entrances(rooms, report):
    for room in rooms.values():
        for door in room.doors:
            if door.entrance is None and door.inline is not None:
                door.entrance = SimpleNamespace(
                    x=door.inline.x,
                    y=door.inline.y,
                    z=door.inline.z,
                    d=door.inline.d,
                    cut=None,
                )
                report['doors from inline entrances'] += 1


def assign_cuts(rooms, report):
    for (dir_id, door_id), poc in sorted(PROOF_OF_CONCEPT.items()):
        matches = [
            door
            for room in rooms.values()
            if key_base(room.key).lower() == dir_id
            for door in room.doors
            if door.id == door_id
        ]
        if not matches:
            fail(f'proof of concept {dir_id}:{door_id}', 'no door in staging has that id')
        x, y, z, d, cut = poc
        for door in matches:
            if door.entrance is not None and coords(door.entrance) == (x, y, z, d):
                door.cut = cut
                report['proof-of-concept cuts used'] += 1
            else:
                door.entrance_comments.append(
                    f'proof-of-concept entrance x={x} y={y} z={z} d={d} cut={cut} not used'
                )
                report['proof-of-concept entrances not used'] += 1
    report['proof-of-concept entrances'] = len(PROOF_OF_CONCEPT)
    for room in rooms.values():
        for door in room.doors:
            if door.cut is None and door.entrance is not None and door.inline is not None:
                if coords(door.inline) == coords(door.entrance) and door.inline.cut is not None:
                    door.cut = door.inline.cut
                    report['cuts from inline entrances'] += 1
            if door.cut is not None:
                report['cuts'] += 1
            if door.entrance is not None:
                report['entrances'] += 1


def assign_spots(rooms, enemy_rows, report):
    # The spots of each room directory, in the order of enemy.csv.
    spots = {}
    for n, row in enumerate(enemy_rows, start=2):
        where = f'enemy.csv:{n}'
        rdt, *values = row
        if not RDT_ID.fullmatch(rdt):
            fail(where, f'{rdt!r} is not an RDT id')
        x, y, z, d, include, exclude = values
        if include or exclude:
            fail(where, 'Include and Exclude are not converted; none of staging\'s rows has them')
        spot = SimpleNamespace(
            x=csv_int(x, where), y=csv_int(y, where), z=csv_int(z, where), d=csv_int(d, where),
        )
        spots.setdefault(dir_of_rdt(rooms, rdt), []).append(spot)
        report['spots'] += 1
    return spots


def check_references(rooms, catalogue, groups, begin_end):
    flag_names = {f.name for room in rooms.values() for f in room.flags}
    for room in rooms.values():
        for door in room.doors:
            where = f'{room.key} door {door.id}'
            check_atoms(door.requires, where, catalogue, rooms, flag_names)
        for item in room.items:
            check_atoms(item.requires, f'{room.key} item {item.id}', catalogue, rooms, flag_names)
        for flag in room.flags:
            check_atoms(flag.requires, f'{room.key} flag {flag.name}', catalogue, rooms, flag_names)
    for start, end in begin_end:
        for key in (start, end):
            if key not in rooms:
                fail('rdt.json beginEndRooms', f'{key} is not a room')


def check_atoms(atoms, where, catalogue, rooms, flag_names):
    for kind, value in atoms:
        if kind == 'item' and int(value) not in catalogue:
            fail(where, f'item({value}) is not in the item catalogue')
        if kind == 'room' and value not in rooms:
            fail(where, f'room({value}) is not a room')
        if kind == 'flag' and value not in flag_names:
            fail(where, f'flag({value}) is not a flag of any room')


# Writing XML.

class Element:
    def __init__(self, tag, attrs=()):
        self.tag = tag
        self.attrs = [(name, str(value)) for name, value in attrs if value is not None]
        self.children = []


class Comment:
    def __init__(self, text):
        if '--' in text or text.endswith('-'):
            raise ConvertError(f'a comment cannot hold {text!r}')
        self.text = text


def boolean(value):
    if value is None:
        return None
    return 'true' if value else 'false'


def words(values):
    return ' '.join(str(v) for v in values) if values else None


def escape(value):
    return (
        value.replace('&', '&amp;')
        .replace('<', '&lt;')
        .replace('>', '&gt;')
        .replace('"', '&quot;')
    )


def render(node, depth, lines):
    pad = '  ' * depth
    if isinstance(node, Comment):
        lines.append(f'{pad}<!-- {node.text} -->')
        return
    attrs = ''.join(f' {name}="{escape(value)}"' for name, value in node.attrs)
    if not node.children:
        lines.append(f'{pad}<{node.tag}{attrs}/>')
        return
    lines.append(f'{pad}<{node.tag}{attrs}>')
    for child in node.children:
        render(child, depth + 1, lines)
    lines.append(f'{pad}</{node.tag}>')


def document(root):
    lines = ['<?xml version="1.0" encoding="UTF-8"?>']
    render(root, 0, lines)
    return '\n'.join(lines) + '\n'


def map_document(begin_end, catalogue, groups):
    root = Element('map')
    for start, end in begin_end:
        root.children.append(Element('begin-end', [('start', start), ('end', end)]))
    items = Element('items')
    root.children.append(items)
    for item in catalogue.values():
        items.children.append(Element('item', [
            ('id', item.id),
            ('name', item.name),
            ('kind', item.kind),
            ('max', item.max),
            ('ammo', words(item.ammo)),
            ('group', item.group),
            ('amount', item.amount),
            ('discard', boolean(item.discard)),
        ]))
    enemies = Element('enemies')
    root.children.append(enemies)
    for group in groups:
        g = Element('group', [
            ('name', group.name),
            ('background', group.background),
            ('foreground', group.foreground),
        ])
        enemies.children.append(g)
        for entry in group.entries:
            e = Element('entry', [
                ('key', entry.key),
                ('name', entry.name),
                ('id', words(entry.ids)),
                ('esp', words(entry.esp)),
            ])
            for pose in entry.poses:
                e.children.append(Element('pose', [
                    ('name', pose.name),
                    ('pose', pose.pose),
                    ('probability', repr(pose.probability)),
                ]))
            g.children.append(e)
    return root


def door_element(door):
    element = Element('door', [
        ('id', door.id),
        ('name', door.name),
        ('target', door.target),
        ('kind', door.kind),
        ('lock', door.lock),
        ('key', door.key),
        ('allowed-locks', None if door.allowed is None else words(door.allowed) or ''),
        ('no-unlock', boolean(door.no_unlock)),
        ('randomize', boolean(door.randomize)),
        ('requires', words([f'{k}({v})' for k, v in door.requires])),
        ('tags', words(door.tags)),
        ('door-rando', boolean(door.door_rando)),
        ('player', door.player),
    ])
    for text in door.entrance_comments:
        element.children.append(Comment(text))
    if door.entrance is not None:
        element.children.append(Element('entrance', [
            ('x', door.entrance.x),
            ('y', door.entrance.y),
            ('z', door.entrance.z),
            ('d', door.entrance.d),
            ('cut', door.cut),
        ]))
    return element


def room_element(room):
    element = Element('room', [
        ('key', room.key),
        ('name', room.name),
        ('tags', words(room.tags)),
    ])
    for door in room.doors:
        for text in door.comments:
            element.children.append(Comment(text))
        element.children.append(door_element(door))
    for flag in room.flags:
        element.children.append(Element('flag', [
            ('name', flag.name),
            ('requires', words([f'{k}({v})' for k, v in flag.requires])),
        ]))
    for item in room.items:
        if item.type_offsets:
            element.children.append(Comment(f'staging typeOffsets: {" ".join(item.type_offsets)}'))
        element.children.append(Element('item', [
            ('id', item.id),
            ('flag', item.flag),
            ('type', item.kind),
            ('amount', item.amount),
            ('group', item.group),
            ('name', item.name),
            ('requires', words([f'{k}({v})' for k, v in item.requires])),
            ('document', boolean(item.document)),
            ('hidden', boolean(item.hidden)),
            ('optional', boolean(item.optional)),
            ('lock', item.lock),
            ('door-rando', boolean(item.door_rando)),
            ('player', item.player),
        ]))
    for rule in room.enemies:
        if rule.condition is not None:
            element.children.append(Comment(f'staging condition: {rule.condition}'))
        if rule.name is not None:
            element.children.append(Comment(f'staging name: {rule.name}'))
        if rule.ignore_ratio is not None:
            element.children.append(Comment(f'staging ignoreRatio: {boolean(rule.ignore_ratio)}'))
        element.children.append(Element('enemies', [
            ('id', rule.id),
            ('flag', rule.flag),
            ('kind', rule.kind),
            ('include', words(rule.include)),
            ('exclude', words(rule.exclude)),
            ('max-difficulty', rule.max_difficulty),
            ('restricted', boolean(rule.restricted)),
            ('player', rule.player),
        ]))
    return element


def spot_element(spot):
    return Element('spot', [
        ('x', spot.x),
        ('y', spot.y),
        ('z', spot.z),
        ('d', spot.d),
    ])


# The conversion.

def convert(meta):
    report = Counter()
    dropped = Counter()
    staging = read_json(os.path.join(meta, 'rdt.json'))
    check_fields(staging, 'rdt.json', TOP_FIELDS, DROPPED_TOP_FIELDS)
    count_dropped(staging, DROPPED_TOP_FIELDS, dropped, 'rdt.json')

    catalogue = read_catalogue(staging)
    groups = read_enemy_groups(staging)
    begin_end = []
    for i, obj in enumerate(staging['beginEndRooms']):
        where = f'rdt.json beginEndRooms[{i}]'
        check_fields(obj, where, BEGIN_END_FIELDS)
        begin_end.append((
            str_field(obj, 'start', where, required=True),
            str_field(obj, 'end', where, required=True),
        ))

    rooms = read_rooms(staging, catalogue, dropped)
    check_references(rooms, catalogue, groups, begin_end)

    door_rows = read_csv_rows(os.path.join(meta, 'doors.csv'), ['Target', 'X', 'Y', 'Z', 'D'])
    enemy_rows = read_csv_rows(
        os.path.join(meta, 'enemy.csv'),
        ['Room', 'X', 'Y', 'Z', 'D', 'Include', 'Exclude'],
        trailing_optional=2,
    )
    report['doors.csv rows'] = len(door_rows)
    report['enemy.csv rows'] = len(enemy_rows)

    assign_doors(rooms, door_rows, report)
    assign_inline_entrances(rooms, report)
    assign_cuts(rooms, report)
    resolve_targets(rooms, report)
    spots = assign_spots(rooms, enemy_rows, report)

    # Every room's directory has a file, and every logical room is in one.
    dirs = sorted(d for d in os.listdir(ROOM_DIR) if os.path.isdir(os.path.join(ROOM_DIR, d)))
    for room in rooms.values():
        if key_base(room.key).lower() not in dirs:
            fail(f'rdt.json rooms.{room.key}', 'there is no room directory for it')

    files = {MAP_FILE: document(map_document(begin_end, catalogue, groups))}
    for d in dirs:
        root = Element('random')
        for room in rooms.values():
            if key_base(room.key).lower() == d:
                root.children.append(room_element(room))
        for spot in spots.get(d, []):
            root.children.append(spot_element(spot))
        files[os.path.join(ROOM_DIR, d, 'random.xml')] = document(root)

    for text in files.values():
        ElementTree.fromstring(text)  # The output must be well-formed XML.

    report['logical rooms'] = len(rooms)
    report['room directories'] = len(dirs)
    report['doors'] = sum(len(r.doors) for r in rooms.values())
    report['room items'] = sum(len(r.items) for r in rooms.values())
    report['catalogue items'] = len(catalogue)
    report['enemy groups'] = len(groups)
    report['enemy entries'] = sum(len(g.entries) for g in groups)
    report['enemy poses'] = sum(len(e.poses) for g in groups for e in g.entries)
    report['enemy rules'] = sum(len(r.enemies) for r in rooms.values())
    report['flags'] = sum(len(r.flags) for r in rooms.values())
    report['begin-end'] = len(begin_end)
    report['doors with allowed-locks=""'] = sum(
        1 for r in rooms.values() for d in r.doors if d.allowed == []
    )
    return files, report, dropped


def write_files(files):
    changed = 0
    for path, text in files.items():
        os.makedirs(os.path.dirname(path), exist_ok=True)
        try:
            with open(path, encoding='utf-8', newline='') as f:
                if f.read() == text:
                    continue
        except FileNotFoundError:
            pass
        with open(path, 'w', encoding='utf-8', newline='') as f:
            f.write(text)
        changed += 1
    return changed


def main(argv):
    if len(argv) != 2:
        print('usage: python3 -I scripts/biorand-convert-rdt.py <path to meta/re1>', file=sys.stderr)
        return 2
    try:
        files, report, dropped = convert(argv[1])
    except ConvertError as error:
        print(f'error: {error}', file=sys.stderr)
        return 1
    changed = write_files(files)
    for label, value in report.items():
        print(f'{label}: {value}')
    for label, value in sorted(dropped.items()):
        print(f'dropped {label}: {value}')
    print(f'files written: {changed} of {len(files)}')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
