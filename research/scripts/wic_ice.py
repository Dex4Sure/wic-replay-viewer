#!/usr/bin/env python3
"""Read the narrow ICE structures needed for replay attribution research.

World in Conflict's ``ice0010`` files are typed field streams. This module does
not claim to be a general ICE decoder. It recognizes only structures verified in
the shipped server data:

* the fixed 40-field prop-type records in ``maps/propsdatabase2.ice``;
* ``WicedPropInstance`` records in map ICE files;
* the fixed 40-field cloud-type records nested under support definitions;
* text and Vector3 fields used for instance name, type, position, and rotation;
* rigid-body boxes used by the server to derive bridge-collapse kill bounds.

Unknown fields stay opaque. Every reader is bounded to one record so incidental
hash bytes elsewhere in a file cannot be promoted into metadata.
"""

from __future__ import annotations

import math
import pathlib
import struct
from dataclasses import dataclass

from wic_bintag import name_hash
from wic_sdf import SdfArchive

ICE_SIGNATURE = b"ice0010\0"

TYPE_TEXT = name_hash("TEXT")
# Observed scalar-real wire type. Its plaintext type name is not present in the
# shipped binary string table, so retain the hash instead of guessing a spelling.
TYPE_REAL = 0x07A901F0
TYPE_VECTOR3 = name_hash("Vector3")
TYPE_WICED_PROP_INSTANCE = name_hash("WicedPropInstance")

# This type hash begins every one of the 1,195 fixed 40-field records in the
# shipped props database. No matching plaintext name has yet been recovered, so
# retain the observed wire identity instead of assigning a guessed class name.
TYPE_PROP_DEFINITION = 0x1189039C
PROP_DEFINITION_FIELD_COUNT = 40
WICED_PROP_INSTANCE_FIELD_COUNT = 11

# Ghidra ``wic_ds.exe:0x00680fb0`` compares this exact runtime type before
# constructing ``EXCO_CloudType``. Every shipped cloud record has 40 fields.
TYPE_CLOUD_DEFINITION = 0x0C8C02F7
CLOUD_DEFINITION_FIELD_COUNT = 40

FIELD_MY_NAME = name_hash("myName")
FIELD_MY_TYPE = name_hash("myType")
FIELD_MY_UNIT_CATEGORY = name_hash("myUnitCategory")
FIELD_MY_POSITION = name_hash("myPosition")
FIELD_MY_HPB = name_hash("myHPB")
FIELD_MY_REPAIR_LINKS = name_hash("myRepairLinks")
FIELD_MY_PHYS_FILE = name_hash("myPhysFile")
FIELD_MY_ROOT_POSITION = name_hash("myRootPosition")
FIELD_MY_POS = name_hash("myPos")
FIELD_MY_ROT_HPB = name_hash("myRotHPB")
FIELD_MY_EXTENTS = name_hash("myExtents")
FIELD_MY_PARASITES = name_hash("myParasites")
FIELD_MY_HEALTH = name_hash("myHealth")
FIELD_MY_SPEED = name_hash("mySpeed")
FIELD_MY_MAX_SPEED_MULTIPLIER = name_hash("myMaxSpeedMultiplier")
FIELD_MY_TIME_TO_LIVE = name_hash("myTimeToLive")
FIELD_MY_HEALTH_CHANGE = name_hash("myHealthChange")
FIELD_MY_HEALTH_CHANGE_INTERVAL = name_hash("myHealthChangeInterval")
FIELD_MY_RADIUS = name_hash("myRadius")
FIELD_MY_AFFECT_FRIENDLY_FLAG = name_hash("myAffectFriendlyFlag")
FIELD_MY_AFFECT_ENEMY_FLAG = name_hash("myAffectEnemyFlag")
FIELD_MY_AFFECTS_INFANTRY_FLAG = name_hash("myAffectsInfantryFlag")
FIELD_MY_AFFECTS_VEHICLE_FLAG = name_hash("myAffectsVehicleFlag")
FIELD_MY_AFFECTS_TANKS_FLAG = name_hash("myAffectsTanksFlag")
FIELD_MY_AFFECTS_COPTERS_FLAG = name_hash("myAffectsCoptersFlag")
FIELD_MY_AFFECTS_MISC_FLAG = name_hash("myAffectsMiscFlag")
FIELD_MY_AFFECTS_BUILDINGS_FLAG = name_hash("myAffectsBuildingsFlag")
FIELD_MY_INITIAL_LOGIC_DELAY = name_hash("myInitialLogicDelay")
FIELD_MY_INFANTRY_DAMAGE_MULTIPLIER = name_hash("myInfantryDamageTakenMultiplier")
FIELD_MY_GROUND_DAMAGE_MULTIPLIER = name_hash("myGroundDamageTakenMultiplier")
FIELD_MY_HEAVY_ARMOR_DAMAGE_MULTIPLIER = name_hash("myHeavyArmorDamageTakenMultiplier")
FIELD_MY_AIR_DAMAGE_MULTIPLIER = name_hash("myAirDamageTakenMultiplier")
FIELD_MY_BUILDING_DAMAGE_MULTIPLIER = name_hash("myBuildingDamageTakenMultiplier")
FIELD_MY_NUMBER_OF_PROJECTILES = name_hash("myNumberOfProjectiles")
FIELD_MY_PROJECTILE_TYPE = name_hash("myProjectileType")
FIELD_MY_PROJECTILE = name_hash("myProjectile")
FIELD_MY_MODEL_FILE = name_hash("myModelFile")
FIELD_MY_POST_HIT_TIME_TO_LIVE = name_hash("myPostHitTimeToLive")
FIELD_MY_HIT_EFFECT = name_hash("myHitEffect")

# Exact plaintext identities recovered from the shipped executable string table.
# Unknown effect node types remain represented by their wire hashes.
PROJECTILE_EFFECT_TYPE_NAMES = {
    name_hash("PP_DirectDamage"): "PP_DirectDamage",
    name_hash("PP_BlastDamage"): "PP_BlastDamage",
    TYPE_CLOUD_DEFINITION: "EXCO_CloudType",
    name_hash("PP_UnitSpawner"): "PP_UnitSpawner",
    name_hash("PP_ForestDestroyer"): "PP_ForestDestroyer",
    name_hash("PP_BloomBurner"): "PP_BloomBurner",
}

# ``wic_ds.exe:0x004791b0`` hashes these strings in this exact order and stores
# the result of the lookup at unit-type offset ``+0xfc``. The EXG_Unit vtable
# method used by ``EXG_Cloud::Update`` returns that value when selecting the
# cloud's damage multiplier.
UNIT_META_TYPES = {
    "AIR": 0,
    "GROUND": 1,
    "INFANTRY": 2,
    "HEAVYARMOR": 3,
}

# ``wic_ds.exe:0x00479230`` performs the corresponding lookup for
# ``myUnitCategory`` and stores it at ``+0x100``. ``EXG_Cloud::Update`` indexes
# its seven category flags with this exact value before applying damage.
UNIT_CATEGORIES = {
    "INFANTRY": 0,
    "VEHICLES": 1,
    "TANKS": 2,
    "COPTERS": 3,
    "MISC": 4,
    "DEEP_WATER": 5,
    "NONTARGETABLE": 6,
}


class IceError(ValueError):
    """Raised when a recognized ICE structure is malformed."""


@dataclass(frozen=True)
class IceRecord:
    """One bounded typed record in an ICE stream."""

    offset: int
    end: int
    key_hash: int
    field_count: int


@dataclass(frozen=True)
class IceNode:
    """One recursively bounded node in an ICE tree."""

    offset: int
    end: int
    type_hash: int
    key_hash: int
    value: bytes | None
    children: tuple["IceNode", ...]


@dataclass(frozen=True)
class PropInstance:
    """Replay-relevant identity and placement of one shipped map prop."""

    name: str
    type_name: str
    name_hash: int
    type_hash: int
    position: tuple[float, float, float]
    hpb_degrees: tuple[float, float, float]
    record_offset: int


@dataclass(frozen=True)
class BridgeInstance(PropInstance):
    """A shipped bridge and the server's exact horizontal fatal bounds."""

    kill_bounds_xz: tuple[float, float, float, float]


@dataclass(frozen=True)
class UnitTypeParasites:
    """Shipped unit-definition identity and its direct parasite classes."""

    name: str
    type_hash: int
    meta_type: int
    meta_type_name: str
    unit_category: int
    unit_category_name: str
    max_health: int
    max_speed: float
    max_speed_multiplier: float
    parasite_type_hashes: frozenset[int]


@dataclass(frozen=True)
class CloudTypeDefinition:
    """One globally indexed cloud loaded from the shipped support catalogue.

    ``wic_ds.exe:0x00681330`` loads support clouds before unit/ammunition
    clouds, ``0x00680fb0`` walks them in serialized order, and ``0x00680f10``
    appends each type to the global array. ``CreateCloud.aType`` is therefore
    this zero-based index for the bounded support-catalogue prefix.
    """

    index: int
    support_id: int
    support_name: str
    support_section: str
    cloud_key_hash: int
    time_to_live: float
    health_change: int
    health_change_interval: float
    radius: float
    initial_logic_delay: float
    affect_friendly: bool
    affect_enemy: bool
    affects_infantry: bool
    affects_vehicle: bool
    affects_tanks: bool
    affects_copters: bool
    affects_misc: bool
    affects_buildings: bool
    infantry_damage_multiplier: float
    ground_damage_multiplier: float
    heavy_armor_damage_multiplier: float
    air_damage_multiplier: float
    building_damage_multiplier: float


@dataclass(frozen=True)
class ProjectileEffectDefinition:
    """One ordered projectile death parasite from a shipped support definition."""

    type_hash: int
    key_hash: int
    type_name: str | None
    scalar_fields: tuple[tuple[int, str], ...]


@dataclass(frozen=True)
class SupportProjectileDefinition:
    """The replay-relevant projectile and effect bundle for one support type."""

    support_id: int
    support_name: str
    support_section: str
    mover_kind: str
    projectile_count: int
    model_file: str
    post_hit_time_to_live: float
    hit_effect: str
    effects: tuple[ProjectileEffectDefinition, ...]


def _u32(data: bytes, offset: int, end: int, label: str) -> int:
    if offset < 0 or offset + 4 > end:
        raise IceError(f"{label} extends outside its ICE record")
    return struct.unpack_from("<I", data, offset)[0]


def _ice_node(data: bytes, offset: int, depth: int = 0) -> tuple[IceNode, int]:
    """Decode one recursive ICE node without interpreting unknown hashes."""
    if depth > 64:
        raise IceError("ICE nesting exceeds the bounded recursion limit")
    start = offset
    if offset + 12 > len(data):
        raise IceError(f"ICE node header extends outside the file at 0x{start:x}")
    type_hash, key_hash, count = struct.unpack_from("<III", data, offset)
    offset += 12
    if count == 0xFFFFFFFF:
        length = _u32(data, offset, len(data), "ICE scalar length")
        offset += 4
        if length > len(data) - offset:
            raise IceError(f"ICE scalar extends outside the file at 0x{start:x}")
        end = offset + length
        return IceNode(start, end, type_hash, key_hash, data[offset:end], ()), end
    if count > (len(data) - offset) // 12:
        raise IceError(f"implausible ICE child count {count} at 0x{start:x}")
    children = []
    for _ in range(count):
        child, offset = _ice_node(data, offset, depth + 1)
        children.append(child)
    return IceNode(start, offset, type_hash, key_hash, None, tuple(children)), offset


def ice_tree_roots(data: bytes) -> tuple[IceNode, ...]:
    """Decode the complete bounded tree used by shipped ICE catalogues."""
    if not data.startswith(ICE_SIGNATURE):
        raise IceError("missing ice0010 signature")
    if len(data) < 16:
        raise IceError("truncated ICE tree header")
    # Shipped catalogues use one top-level collection followed by its root count.
    if _u32(data, 8, len(data), "ICE collection count") != 1:
        raise IceError("unsupported ICE top-level collection count")
    root_count = _u32(data, 12, len(data), "ICE root count")
    if root_count > (len(data) - 16) // 12:
        raise IceError("implausible ICE root count")
    offset = 16
    roots = []
    for _ in range(root_count):
        root, offset = _ice_node(data, offset)
        roots.append(root)
    if offset != len(data):
        raise IceError(f"unparsed ICE bytes remain at 0x{offset:x}")
    return tuple(roots)


def _scalar_child(node: IceNode, field_hash: int) -> bytes:
    matches = [child for child in node.children if child.key_hash == field_hash]
    if len(matches) != 1 or matches[0].value is None:
        raise IceError(
            f"ICE node at 0x{node.offset:x} has no unique scalar "
            f"field 0x{field_hash:08x}"
        )
    return matches[0].value


def _ascii_int_child(node: IceNode, field_hash: int) -> int:
    try:
        return int(_scalar_child(node, field_hash).decode("ascii"))
    except (UnicodeDecodeError, ValueError) as error:
        raise IceError(
            f"ICE field 0x{field_hash:08x} is not an ASCII integer"
        ) from error


def _ascii_float_child(node: IceNode, field_hash: int) -> float:
    try:
        return float(_scalar_child(node, field_hash).decode("ascii"))
    except (UnicodeDecodeError, ValueError) as error:
        raise IceError(f"ICE field 0x{field_hash:08x} is not an ASCII real") from error


def _ascii_bool_child(node: IceNode, field_hash: int) -> bool:
    value = _ascii_int_child(node, field_hash)
    if value not in (0, 1):
        raise IceError(f"ICE boolean field 0x{field_hash:08x} is {value}, not 0/1")
    return value == 1


def _walk_ice_nodes(node: IceNode):
    yield node
    for child in node.children:
        yield from _walk_ice_nodes(child)


def _recursive_scalar_values(node: IceNode, field_hash: int) -> list[bytes]:
    return [
        candidate.value
        for candidate in _walk_ice_nodes(node)
        if candidate.key_hash == field_hash and candidate.value is not None
    ]


def _records(
    data: bytes,
    type_hash: int,
    expected_field_count: int,
) -> list[IceRecord]:
    """Find fixed-shape top-level records and bound each by the next record."""
    if not data.startswith(ICE_SIGNATURE):
        raise IceError("missing ice0010 signature")
    marker = struct.pack("<I", type_hash)
    starts = []
    cursor = 0
    while True:
        offset = data.find(marker, cursor)
        if offset < 0:
            break
        if offset + 12 <= len(data):
            key_hash, field_count = struct.unpack_from("<II", data, offset + 4)
            if field_count == expected_field_count:
                starts.append((offset, key_hash, field_count))
        cursor = offset + 4
    return [
        IceRecord(
            offset=offset,
            end=starts[index + 1][0] if index + 1 < len(starts) else len(data),
            key_hash=key_hash,
            field_count=field_count,
        )
        for index, (offset, key_hash, field_count) in enumerate(starts)
    ]


def _field_offset(
    data: bytes,
    record: IceRecord,
    field_type: int,
    field_name: int,
) -> int:
    marker = struct.pack("<II", field_type, field_name)
    offset = data.find(marker, record.offset + 12, record.end)
    if offset < 0:
        raise IceError(
            f"field 0x{field_name:08x} is absent from record at 0x{record.offset:x}"
        )
    # Some prop instances contain nested parameter records that legitimately
    # repeat generic names such as ``myType``. The instance's own fixed fields
    # occur first, so select the first bounded match.
    return offset


def _text_field(data: bytes, record: IceRecord, field_name: int) -> str:
    offset = _field_offset(data, record, TYPE_TEXT, field_name)
    if _u32(data, offset + 8, record.end, "text null marker") != 0xFFFFFFFF:
        raise IceError("recognized text field has an unexpected null marker")
    length = _u32(data, offset + 12, record.end, "text length")
    start = offset + 16
    if length > record.end - start:
        raise IceError("text value extends outside its ICE record")
    try:
        return data[start : start + length].decode("utf-8")
    except UnicodeDecodeError as error:
        raise IceError("text value is not UTF-8/ASCII") from error


def _vector3_field(
    data: bytes,
    record: IceRecord,
    field_name: int,
) -> tuple[float, float, float]:
    offset = _field_offset(data, record, TYPE_VECTOR3, field_name)
    if _u32(data, offset + 8, record.end, "Vector3 component count") != 3:
        raise IceError("recognized Vector3 does not have three components")
    cursor = offset + 12
    values = []
    for index in range(3):
        if (
            _u32(data, cursor, record.end, f"Vector3 component {index} type")
            != TYPE_REAL
        ):
            raise IceError("Vector3 component is not encoded as Real")
        if (
            _u32(data, cursor + 8, record.end, f"Vector3 component {index} null marker")
            != 0xFFFFFFFF
        ):
            raise IceError("Vector3 component has an unexpected null marker")
        length = _u32(
            data, cursor + 12, record.end, f"Vector3 component {index} length"
        )
        start = cursor + 16
        if length > record.end - start:
            raise IceError("Vector3 component extends outside its ICE record")
        try:
            values.append(float(data[start : start + length].decode("ascii")))
        except (UnicodeDecodeError, ValueError) as error:
            raise IceError("Vector3 component is not an ASCII real") from error
        cursor = start + length
    return tuple(values)


def _vector3_child(node: IceNode, field_hash: int) -> tuple[float, float, float]:
    matches = [child for child in node.children if child.key_hash == field_hash]
    if len(matches) != 1 or len(matches[0].children) != 3:
        raise IceError(
            f"ICE node at 0x{node.offset:x} has no unique Vector3 "
            f"field 0x{field_hash:08x}"
        )
    values = []
    for component in matches[0].children:
        if component.value is None:
            raise IceError(f"ICE Vector3 field 0x{field_hash:08x} is not scalar")
        try:
            values.append(float(component.value.decode("ascii")))
        except (UnicodeDecodeError, ValueError) as error:
            raise IceError(
                f"ICE Vector3 field 0x{field_hash:08x} is not ASCII real"
            ) from error
    return tuple(values)


def _physics_roots(data: bytes) -> tuple[IceNode, ...]:
    """Decode the bounded root sequence used by shipped rigid-body ICE files."""
    if not data.startswith(ICE_SIGNATURE):
        raise IceError("missing ice0010 signature")
    if len(data) < 16:
        raise IceError("truncated physics ICE header")
    if _u32(data, 8, len(data), "physics ICE marker") != 0:
        raise IceError("unsupported physics ICE marker")
    root_count = _u32(data, 12, len(data), "physics ICE root count")
    if root_count > (len(data) - 16) // 12:
        raise IceError("implausible physics ICE root count")
    offset = 16
    roots = []
    for _ in range(root_count):
        root, offset = _ice_node(data, offset)
        roots.append(root)
    if offset != len(data):
        raise IceError(f"unparsed physics ICE bytes remain at 0x{offset:x}")
    return tuple(roots)


def _direct_child(node: IceNode, field_hash: int) -> IceNode | None:
    matches = [child for child in node.children if child.key_hash == field_hash]
    if len(matches) > 1:
        raise IceError(
            f"ICE node at 0x{node.offset:x} repeats field 0x{field_hash:08x}"
        )
    return matches[0] if matches else None


def _rigid_body_boxes(
    data: bytes,
) -> tuple[
    tuple[
        tuple[float, float, float],
        tuple[float, float, float],
        tuple[float, float, float],
        tuple[float, float, float],
    ],
    ...,
]:
    """Return ``(root, pos, hpb-radians, extents)`` for box elements.

    ``wic_ds.exe:0x0067d760`` accepts rigid-body element type zero as an
    oriented box. Bridge destruction bounds at ``0x0051c630`` are derived from
    those boxes; silently accepting another element shape would therefore
    change the server rule.
    """
    boxes = []
    for node in (
        item for root in _physics_roots(data) for item in _walk_ice_nodes(root)
    ):
        if _direct_child(node, FIELD_MY_ROOT_POSITION) is None:
            continue
        root_position = _vector3_child(node, FIELD_MY_ROOT_POSITION)
        for element in _walk_ice_nodes(node):
            if element is node:
                continue
            position = _direct_child(element, FIELD_MY_POS)
            rotation = _direct_child(element, FIELD_MY_ROT_HPB)
            extents = _direct_child(element, FIELD_MY_EXTENTS)
            if position is None and rotation is None and extents is None:
                continue
            if position is None or rotation is None or extents is None:
                raise IceError(
                    f"partial rigid-body box at ICE offset 0x{element.offset:x}"
                )
            type_field = _direct_child(element, FIELD_MY_TYPE)
            if type_field is None or type_field.value is None:
                raise IceError(
                    f"rigid-body element at 0x{element.offset:x} lacks scalar myType"
                )
            try:
                element_type = int(type_field.value.decode("ascii"))
            except (UnicodeDecodeError, ValueError) as error:
                raise IceError(
                    "rigid-body element type is not an ASCII integer"
                ) from error
            if element_type != 0:
                raise IceError(
                    f"unsupported rigid-body element type {element_type} "
                    f"at 0x{element.offset:x}"
                )
            boxes.append(
                (
                    root_position,
                    _vector3_child(element, FIELD_MY_POS),
                    _vector3_child(element, FIELD_MY_ROT_HPB),
                    _vector3_child(element, FIELD_MY_EXTENTS),
                )
            )
    if not boxes:
        raise IceError("physics ICE has no rigid-body box elements")
    return tuple(boxes)


def bridge_kill_bounds_xz(
    physics_data: bytes,
    position: tuple[float, float, float],
    hpb_degrees: tuple[float, float, float],
) -> tuple[float, float, float, float]:
    """Reproduce the horizontal bridge fatal AABB from server build b35.

    The map loader at ``wic_ds.exe:0x004e2c00`` converts instance degrees with
    its literal ``3.14 / 180`` factor. ``0x0051c630`` transforms the four X/Z
    corners of every oriented rigid-body box by the instance heading and keeps
    their global minima/maxima. ``0x004d37c0`` later applies strict containment
    against exactly these four bounds when a bridge collapses.
    """
    heading = hpb_degrees[0] * (3.14 / 180.0)
    heading_cos = math.cos(heading)
    heading_sin = math.sin(heading)
    corners = []
    for root, local_position, rotation, extents in _rigid_body_boxes(physics_data):
        h, p, b = rotation
        ch, sh = math.cos(h), math.sin(h)
        cp, sp = math.cos(p), math.sin(p)
        cb, sb = math.cos(b), math.sin(b)
        # Matrix layout and row-vector use follow 0x0067d760/0x0051c630.
        m00 = sp * sh * sb + cb * ch
        m02 = sp * ch * sb - cb * sh
        m10 = sp * cb * sh - ch * sb
        m12 = sp * cb * ch + sh * sb
        m20 = cp * sh
        m22 = cp * ch
        center_x = root[0] + local_position[0]
        center_z = root[2] + local_position[2]
        extent_x, extent_y, extent_z = extents
        for sign_x, sign_z in ((1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)):
            local_x = center_x - (
                sign_x * extent_x * m00 + 0.0 * extent_y * m10 + sign_z * extent_z * m20
            )
            local_z = center_z - (
                sign_x * extent_x * m02 + 0.0 * extent_y * m12 + sign_z * extent_z * m22
            )
            corners.append(
                (
                    position[0] + local_z * heading_sin + local_x * heading_cos,
                    position[2] + local_z * heading_cos - local_x * heading_sin,
                )
            )
    return (
        min(point[0] for point in corners),
        min(point[1] for point in corners),
        max(point[0] for point in corners),
        max(point[1] for point in corners),
    )


def prop_definition_records(data: bytes) -> list[IceRecord]:
    """Return every fixed prop-definition record from propsdatabase2.ice."""
    return _records(
        data,
        TYPE_PROP_DEFINITION,
        PROP_DEFINITION_FIELD_COUNT,
    )


def bridge_type_hashes(data: bytes) -> set[int]:
    """Return prop-type keys with one or more serialized repair links.

    Ghidra confirms the server constructs ``WICG_Bridge`` only when this list is
    nonempty. The ICE list count immediately follows ``myRepairLinks``; inherited
    or empty values begin with ``0xffffffff`` and are not bridges.
    """
    repair_marker = struct.pack("<I", FIELD_MY_REPAIR_LINKS)
    bridges = set()
    for record in prop_definition_records(data):
        offset = data.find(repair_marker, record.offset + 12, record.end)
        if offset < 0:
            raise IceError(
                f"prop definition at 0x{record.offset:x} lacks myRepairLinks"
            )
        if data.find(repair_marker, offset + 4, record.end) >= 0:
            raise IceError(
                f"prop definition at 0x{record.offset:x} repeats myRepairLinks"
            )
        count_or_default = _u32(data, offset + 4, record.end, "repair-link count")
        if 0 < count_or_default < 0x10000:
            bridges.add(record.key_hash)
    return bridges


def prop_instances(data: bytes) -> list[PropInstance]:
    """Decode all fixed ``WicedPropInstance`` records in one map ICE file."""
    records = _records(
        data,
        TYPE_WICED_PROP_INSTANCE,
        WICED_PROP_INSTANCE_FIELD_COUNT,
    )
    instances = []
    for record in records:
        name = _text_field(data, record, FIELD_MY_NAME)
        type_name = _text_field(data, record, FIELD_MY_TYPE)
        position = _vector3_field(data, record, FIELD_MY_POSITION)
        hpb_degrees = _vector3_field(data, record, FIELD_MY_HPB)
        # Most keys equal Adler32(myName), but some shipped maps deliberately
        # carry a different serialized object key. Replay messages use this
        # record key, so preserve it as authoritative instead of recomputing it.
        instances.append(
            PropInstance(
                name=name,
                type_name=type_name,
                name_hash=record.key_hash,
                type_hash=name_hash(type_name),
                position=position,
                hpb_degrees=hpb_degrees,
                record_offset=record.offset,
            )
        )
    return instances


def archive_entry(archive: SdfArchive, path: str) -> bytes:
    """Read exactly one named ICE entry from an already validated SDF."""
    matches = [entry for entry in archive.entries if entry.path == path]
    if len(matches) != 1:
        raise IceError(f"expected one SDF entry for {path!r}, found {len(matches)}")
    return archive.read_entry(matches[0])


def unit_type_names(data: bytes) -> dict[int, str]:
    """Map exact unit-definition hashes from the paired shipped LOC catalogue."""
    try:
        text = data.decode("utf-8-sig")
    except UnicodeDecodeError as error:
        raise IceError("unit-type localization is not UTF-8") from error
    names: dict[int, str] = {}
    for line in text.splitlines():
        if not line.startswith("UnitTypes.") or line.count(".") < 2:
            continue
        name = line.split(".", 2)[1]
        type_hash = name_hash(name)
        previous = names.setdefault(type_hash, name)
        if previous != name:
            raise IceError(
                f"unit-type localization hash collision: {previous!r} and {name!r}"
            )
    return names


def unit_type_parasites(
    ice_data: bytes,
    localization_data: bytes,
) -> dict[int, UnitTypeParasites]:
    """Return every named top-level unit definition and direct parasite types."""
    names = unit_type_names(localization_data)
    definitions = {}
    for root in ice_tree_roots(ice_data):
        for node in root.children:
            name = names.get(node.key_hash)
            if name is None:
                continue
            try:
                meta_type_name = _scalar_child(node, FIELD_MY_TYPE).decode("ascii")
                unit_category_name = _scalar_child(node, FIELD_MY_UNIT_CATEGORY).decode(
                    "ascii"
                )
            except UnicodeDecodeError as error:
                raise IceError(
                    f"unit definition {name!r} has a non-ASCII type/category"
                ) from error
            if meta_type_name not in UNIT_META_TYPES:
                raise IceError(
                    f"unit definition {name!r} has unknown myType {meta_type_name!r}"
                )
            if unit_category_name not in UNIT_CATEGORIES:
                raise IceError(
                    "unit definition "
                    f"{name!r} has unknown myUnitCategory {unit_category_name!r}"
                )
            parasite_fields = [
                child for child in node.children if child.key_hash == FIELD_MY_PARASITES
            ]
            if len(parasite_fields) != 1:
                raise IceError(
                    f"unit definition {name!r} has {len(parasite_fields)} myParasites fields"
                )
            health_fields = [
                child for child in node.children if child.key_hash == FIELD_MY_HEALTH
            ]
            if len(health_fields) != 1 or health_fields[0].value is None:
                raise IceError(
                    f"unit definition {name!r} has no unique scalar myHealth field"
                )
            try:
                max_health = int(health_fields[0].value.decode("ascii"))
            except (UnicodeDecodeError, ValueError) as error:
                raise IceError(
                    f"unit definition {name!r} has a non-integer myHealth value"
                ) from error
            speed_values = _recursive_scalar_values(node, FIELD_MY_SPEED)
            if len(speed_values) != 1:
                raise IceError(
                    f"unit definition {name!r} has {len(speed_values)} mySpeed fields"
                )
            multiplier_values = _recursive_scalar_values(
                node, FIELD_MY_MAX_SPEED_MULTIPLIER
            )
            try:
                max_speed = float(speed_values[0].decode("ascii"))
                max_speed_multiplier = max(
                    [1.0]
                    + [float(value.decode("ascii")) for value in multiplier_values]
                )
            except (UnicodeDecodeError, ValueError) as error:
                raise IceError(
                    f"unit definition {name!r} has a non-real speed field"
                ) from error
            if max_speed < 0.0 or max_speed_multiplier < 1.0:
                raise IceError(f"unit definition {name!r} has an invalid speed bound")
            definition = UnitTypeParasites(
                name=name,
                type_hash=node.key_hash,
                meta_type=UNIT_META_TYPES[meta_type_name],
                meta_type_name=meta_type_name,
                unit_category=UNIT_CATEGORIES[unit_category_name],
                unit_category_name=unit_category_name,
                max_health=max_health,
                max_speed=max_speed,
                max_speed_multiplier=max_speed_multiplier,
                parasite_type_hashes=frozenset(
                    child.type_hash for child in parasite_fields[0].children
                ),
            )
            previous = definitions.setdefault(node.key_hash, definition)
            if previous != definition:
                raise IceError(f"unit definition {name!r} is serialized inconsistently")
    missing = set(names) - set(definitions)
    if missing:
        missing_names = ", ".join(
            repr(names[type_hash]) for type_hash in sorted(missing)
        )
        raise IceError(f"localized unit definitions absent from ICE: {missing_names}")
    return definitions


def support_cloud_types(
    ice_data: bytes,
    localization_data: bytes,
) -> tuple[CloudTypeDefinition, ...]:
    """Decode the globally indexed support-cloud prefix in exact load order.

    The support catalogue is grouped by faction/presentation class, with each
    support definition stored directly beneath its group. Cloud definitions can
    be nested inside the support's projectile/effect fields, so they are walked
    depth-first in serialized order. This is the same order observed at the
    ``EXCO_CloudType`` loader and cross-checked against serialized
    ``CreateCloud.aType``/``aTimeToLive`` pairs.
    """
    # Keep the localization parser at module-call scope so this narrow ICE
    # reader remains usable without importing unrelated reporting code at
    # startup.
    from ta_support_catalogue import parse_support_localization

    localized = parse_support_localization(localization_data)
    localized_by_hash = {
        name_hash(name): support for name, support in localized.items()
    }
    roots = ice_tree_roots(ice_data)
    if len(roots) != 1:
        raise IceError(f"expected one support catalogue root, found {len(roots)}")

    definitions = []
    for group in roots[0].children:
        # The final scalar catalogue-version field is not a support group.
        if not group.children:
            continue
        for support_node in group.children:
            support = localized_by_hash.get(support_node.key_hash)
            cloud_nodes = [
                node
                for node in _walk_ice_nodes(support_node)
                if node.type_hash == TYPE_CLOUD_DEFINITION
                and len(node.children) == CLOUD_DEFINITION_FIELD_COUNT
            ]
            if not cloud_nodes:
                continue
            if support is None:
                raise IceError(
                    "cloud-bearing support definition is absent from localization: "
                    f"0x{support_node.key_hash:08x}"
                )
            for cloud in cloud_nodes:
                definitions.append(
                    CloudTypeDefinition(
                        index=len(definitions),
                        support_id=support_node.key_hash,
                        support_name=support.internal_name,
                        support_section=support.section,
                        cloud_key_hash=cloud.key_hash,
                        time_to_live=_ascii_float_child(cloud, FIELD_MY_TIME_TO_LIVE),
                        health_change=_ascii_int_child(cloud, FIELD_MY_HEALTH_CHANGE),
                        health_change_interval=_ascii_float_child(
                            cloud, FIELD_MY_HEALTH_CHANGE_INTERVAL
                        ),
                        radius=_ascii_float_child(cloud, FIELD_MY_RADIUS),
                        initial_logic_delay=_ascii_float_child(
                            cloud, FIELD_MY_INITIAL_LOGIC_DELAY
                        ),
                        affect_friendly=_ascii_bool_child(
                            cloud, FIELD_MY_AFFECT_FRIENDLY_FLAG
                        ),
                        affect_enemy=_ascii_bool_child(
                            cloud, FIELD_MY_AFFECT_ENEMY_FLAG
                        ),
                        affects_infantry=_ascii_bool_child(
                            cloud, FIELD_MY_AFFECTS_INFANTRY_FLAG
                        ),
                        affects_vehicle=_ascii_bool_child(
                            cloud, FIELD_MY_AFFECTS_VEHICLE_FLAG
                        ),
                        affects_tanks=_ascii_bool_child(
                            cloud, FIELD_MY_AFFECTS_TANKS_FLAG
                        ),
                        affects_copters=_ascii_bool_child(
                            cloud, FIELD_MY_AFFECTS_COPTERS_FLAG
                        ),
                        affects_misc=_ascii_bool_child(
                            cloud, FIELD_MY_AFFECTS_MISC_FLAG
                        ),
                        affects_buildings=_ascii_bool_child(
                            cloud, FIELD_MY_AFFECTS_BUILDINGS_FLAG
                        ),
                        infantry_damage_multiplier=_ascii_float_child(
                            cloud, FIELD_MY_INFANTRY_DAMAGE_MULTIPLIER
                        ),
                        ground_damage_multiplier=_ascii_float_child(
                            cloud, FIELD_MY_GROUND_DAMAGE_MULTIPLIER
                        ),
                        heavy_armor_damage_multiplier=_ascii_float_child(
                            cloud, FIELD_MY_HEAVY_ARMOR_DAMAGE_MULTIPLIER
                        ),
                        air_damage_multiplier=_ascii_float_child(
                            cloud, FIELD_MY_AIR_DAMAGE_MULTIPLIER
                        ),
                        building_damage_multiplier=_ascii_float_child(
                            cloud, FIELD_MY_BUILDING_DAMAGE_MULTIPLIER
                        ),
                    )
                )
    return tuple(definitions)


def support_projectile_definitions(
    ice_data: bytes,
    localization_data: bytes,
) -> dict[int, tuple[SupportProjectileDefinition, ...]]:
    """Decode every shipped support projectile definition without inferring use.

    This preserves the ordered death-parasite bundle and every direct scalar in
    each parasite.  The catalogue tells us what a projectile will execute when
    it terminates; it does not prove which later replay event was produced by a
    particular projectile.
    """
    from ta_support_catalogue import parse_support_localization

    localized = parse_support_localization(localization_data)
    localized_by_hash = {
        name_hash(name): support for name, support in localized.items()
    }
    roots = ice_tree_roots(ice_data)
    if len(roots) != 1:
        raise IceError(f"expected one support catalogue root, found {len(roots)}")

    definitions: dict[int, list[SupportProjectileDefinition]] = {}
    for group in roots[0].children:
        if not group.children:
            continue
        for support_node in group.children:
            support = localized_by_hash.get(support_node.key_hash)
            if support is None:
                raise IceError(
                    "support definition is absent from localization: "
                    f"0x{support_node.key_hash:08x}"
                )
            try:
                mover_kind = _scalar_child(
                    support_node, FIELD_MY_PROJECTILE_TYPE
                ).decode("ascii")
                projectile_count = _ascii_int_child(
                    support_node, FIELD_MY_NUMBER_OF_PROJECTILES
                )
            except UnicodeDecodeError as error:
                raise IceError(
                    f"support {support.internal_name!r} has a non-ASCII mover kind"
                ) from error
            if mover_kind not in {"STRAIGHT", "BALLISTIC", "HOMING"}:
                raise IceError(
                    f"support {support.internal_name!r} has unknown mover "
                    f"{mover_kind!r}"
                )
            if projectile_count < 0:
                raise IceError(
                    f"support {support.internal_name!r} has a negative projectile count"
                )

            projectile = _direct_child(support_node, FIELD_MY_PROJECTILE)
            parasites = _direct_child(support_node, FIELD_MY_PARASITES)
            if projectile is None or projectile.value is not None:
                raise IceError(
                    f"support {support.internal_name!r} has no unique projectile node"
                )
            if parasites is None or parasites.value not in (None, b""):
                raise IceError(
                    f"support {support.internal_name!r} has no unique parasite list"
                )
            try:
                model_file = _scalar_child(projectile, FIELD_MY_MODEL_FILE).decode(
                    "utf-8"
                )
                hit_effect = _scalar_child(projectile, FIELD_MY_HIT_EFFECT).decode(
                    "utf-8"
                )
            except UnicodeDecodeError as error:
                raise IceError(
                    f"support {support.internal_name!r} has non-UTF-8 projectile text"
                ) from error

            effects = []
            for effect in parasites.children:
                scalar_fields = []
                for field in effect.children:
                    if field.value is None:
                        continue
                    try:
                        value = field.value.decode("utf-8")
                    except UnicodeDecodeError as error:
                        raise IceError(
                            f"support {support.internal_name!r} has a non-UTF-8 "
                            f"effect scalar 0x{field.key_hash:08x}"
                        ) from error
                    scalar_fields.append((field.key_hash, value))
                effects.append(
                    ProjectileEffectDefinition(
                        type_hash=effect.type_hash,
                        key_hash=effect.key_hash,
                        type_name=PROJECTILE_EFFECT_TYPE_NAMES.get(effect.type_hash),
                        scalar_fields=tuple(scalar_fields),
                    )
                )
            definition = SupportProjectileDefinition(
                support_id=support_node.key_hash,
                support_name=support.internal_name,
                support_section=support.section,
                mover_kind=mover_kind,
                projectile_count=projectile_count,
                model_file=model_file,
                post_hit_time_to_live=_ascii_float_child(
                    projectile, FIELD_MY_POST_HIT_TIME_TO_LIVE
                ),
                hit_effect=hit_effect,
                effects=tuple(effects),
            )
            variants = definitions.setdefault(support_node.key_hash, [])
            if definition not in variants:
                variants.append(definition)

    missing = set(localized_by_hash) - set(definitions)
    if missing:
        raise IceError(
            "localized support definitions absent from ICE: "
            + ", ".join(f"0x{value:08x}" for value in sorted(missing))
        )
    return {support_id: tuple(variants) for support_id, variants in definitions.items()}


def shipped_bridge_types(
    archive_path: str | pathlib.Path = "local/binaries/server/wic_ds.sdf",
) -> set[int]:
    """Load the deterministic bridge type-key set from shipped server data."""
    archive = SdfArchive.open(archive_path)
    return bridge_type_hashes(archive_entry(archive, "maps/propsdatabase2.ice"))


def shipped_unit_type_parasites(
    archive_path: str | pathlib.Path = "local/binaries/server/wic_ds.sdf",
) -> dict[int, UnitTypeParasites]:
    """Load exact multiplayer unit parasites from the paired ICE/LOC entries."""
    archive = SdfArchive.open(archive_path)
    return unit_type_parasites(
        archive_entry(archive, "units/unittypes_wic.ice"),
        archive_entry(archive, "units/unittypes_wic.loc"),
    )


def shipped_support_cloud_types(
    archive_path: str | pathlib.Path = "local/binaries/server/wic_ds.sdf",
) -> tuple[CloudTypeDefinition, ...]:
    """Load the exact support-cloud prefix from shipped server data."""
    archive = SdfArchive.open(archive_path)
    return support_cloud_types(
        archive_entry(archive, "maps/supportweapons.ice"),
        archive_entry(archive, "maps/supportweapons.loc"),
    )


def shipped_support_projectile_definitions(
    archive_path: str | pathlib.Path = "local/binaries/server/wic_ds.sdf",
) -> dict[int, tuple[SupportProjectileDefinition, ...]]:
    """Load exact support projectile/effect bundles from shipped server data."""
    archive = SdfArchive.open(archive_path)
    return support_projectile_definitions(
        archive_entry(archive, "maps/supportweapons.ice"),
        archive_entry(archive, "maps/supportweapons.loc"),
    )


def shipped_bridge_instances(
    archive_path: str | pathlib.Path = "local/binaries/server/wic_ds.sdf",
) -> dict[str, dict[int, BridgeInstance]]:
    """Return exact shipped bridge identities and server fatal bounds."""
    archive = SdfArchive.open(archive_path)
    prop_data = archive_entry(archive, "maps/propsdatabase2.ice")
    bridge_types = bridge_type_hashes(prop_data)
    physics_paths = {}
    for record in prop_definition_records(prop_data):
        if record.key_hash not in bridge_types:
            continue
        node, end = _ice_node(prop_data, record.offset)
        if end != record.end:
            raise IceError(
                f"prop definition at 0x{record.offset:x} is not exactly bounded"
            )
        values = [
            value.decode("utf-8")
            for value in _recursive_scalar_values(node, FIELD_MY_PHYS_FILE)
            if value
        ]
        if len(values) > 1:
            raise IceError(
                f"bridge type 0x{record.key_hash:08x} has multiple death physics files"
            )
        if values:
            physics_paths[record.key_hash] = values[0]
    result = {}
    for entry in archive.entries:
        parts = entry.path.split("/")
        if (
            len(parts) != 3
            or parts[0] != "maps"
            or parts[2] != f"{parts[1]}.ice"
            or entry.codec not in (0, 1)
        ):
            continue
        instances = {}
        for instance in prop_instances(archive.read_entry(entry)):
            if instance.type_hash not in bridge_types:
                continue
            physics_path = physics_paths.get(instance.type_hash)
            if physics_path is None:
                raise IceError(
                    f"map bridge type {instance.type_name!r} has no death physics file"
                )
            bounds = bridge_kill_bounds_xz(
                archive_entry(archive, physics_path),
                instance.position,
                instance.hpb_degrees,
            )
            bridge = BridgeInstance(
                name=instance.name,
                type_name=instance.type_name,
                name_hash=instance.name_hash,
                type_hash=instance.type_hash,
                position=instance.position,
                hpb_degrees=instance.hpb_degrees,
                record_offset=instance.record_offset,
                kill_bounds_xz=bounds,
            )
            previous = instances.setdefault(bridge.name_hash, bridge)
            if previous != bridge:
                raise IceError(f"map bridge hash collision at 0x{bridge.name_hash:08x}")
        if instances:
            result[entry.path] = instances
    return result
