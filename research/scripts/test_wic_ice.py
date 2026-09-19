import struct
import unittest

from wic_bintag import name_hash
from wic_ice import (
    CLOUD_DEFINITION_FIELD_COUNT,
    FIELD_MY_AFFECTS_BUILDINGS_FLAG,
    FIELD_MY_AFFECTS_COPTERS_FLAG,
    FIELD_MY_AFFECTS_INFANTRY_FLAG,
    FIELD_MY_AFFECTS_MISC_FLAG,
    FIELD_MY_AFFECTS_TANKS_FLAG,
    FIELD_MY_AFFECTS_VEHICLE_FLAG,
    FIELD_MY_AFFECT_ENEMY_FLAG,
    FIELD_MY_AFFECT_FRIENDLY_FLAG,
    FIELD_MY_AIR_DAMAGE_MULTIPLIER,
    FIELD_MY_BUILDING_DAMAGE_MULTIPLIER,
    FIELD_MY_GROUND_DAMAGE_MULTIPLIER,
    FIELD_MY_HEALTH,
    FIELD_MY_HEALTH_CHANGE,
    FIELD_MY_HEALTH_CHANGE_INTERVAL,
    FIELD_MY_HEAVY_ARMOR_DAMAGE_MULTIPLIER,
    FIELD_MY_HPB,
    FIELD_MY_INFANTRY_DAMAGE_MULTIPLIER,
    FIELD_MY_INITIAL_LOGIC_DELAY,
    FIELD_MY_MAX_SPEED_MULTIPLIER,
    FIELD_MY_NAME,
    FIELD_MY_PARASITES,
    FIELD_MY_POSITION,
    FIELD_MY_NUMBER_OF_PROJECTILES,
    FIELD_MY_PROJECTILE,
    FIELD_MY_PROJECTILE_TYPE,
    FIELD_MY_MODEL_FILE,
    FIELD_MY_POST_HIT_TIME_TO_LIVE,
    FIELD_MY_HIT_EFFECT,
    FIELD_MY_REPAIR_LINKS,
    FIELD_MY_RADIUS,
    FIELD_MY_SPEED,
    FIELD_MY_TIME_TO_LIVE,
    FIELD_MY_TYPE,
    FIELD_MY_UNIT_CATEGORY,
    ICE_SIGNATURE,
    TYPE_PROP_DEFINITION,
    TYPE_CLOUD_DEFINITION,
    TYPE_REAL,
    TYPE_TEXT,
    TYPE_VECTOR3,
    TYPE_WICED_PROP_INSTANCE,
    bridge_kill_bounds_xz,
    bridge_type_hashes,
    ice_tree_roots,
    prop_instances,
    support_cloud_types,
    support_projectile_definitions,
    unit_type_parasites,
)


def text_field(field_hash, value):
    encoded = value.encode()
    return (
        struct.pack("<IIII", TYPE_TEXT, field_hash, 0xFFFFFFFF, len(encoded)) + encoded
    )


def vector3_field(field_hash, values):
    result = struct.pack("<III", TYPE_VECTOR3, field_hash, 3)
    for index, value in enumerate(values):
        encoded = str(value).encode()
        result += (
            struct.pack(
                "<IIII",
                TYPE_REAL,
                name_hash(f"component{index}"),
                0xFFFFFFFF,
                len(encoded),
            )
            + encoded
        )
    return result


def scalar_field(field_hash, value):
    encoded = str(value).encode("ascii")
    return (
        struct.pack("<IIII", TYPE_REAL, field_hash, 0xFFFFFFFF, len(encoded)) + encoded
    )


class IceTests(unittest.TestCase):
    def test_decodes_support_projectile_and_ordered_effect_bundle(self):
        support_name = "Airstrike_Test"
        direct_damage = name_hash("PP_DirectDamage")
        blast_damage = name_hash("PP_BlastDamage")
        projectile = struct.pack(
            "<III", name_hash("ProjectileType"), FIELD_MY_PROJECTILE, 4
        )
        projectile += scalar_field(FIELD_MY_MODEL_FILE, "effects/shell.mrb")
        projectile += scalar_field(FIELD_MY_POST_HIT_TIME_TO_LIVE, "1.25")
        projectile += struct.pack("<III", 0x11111111, 0x22222222, 0)
        projectile += scalar_field(FIELD_MY_HIT_EFFECT, "Support_Test")
        parasites = struct.pack("<III", 0x33333333, FIELD_MY_PARASITES, 2)
        parasites += struct.pack("<III", direct_damage, 0x44444444, 1)
        parasites += scalar_field(name_hash("myDamage"), 1800)
        parasites += struct.pack("<III", blast_damage, 0x55555555, 2)
        parasites += scalar_field(name_hash("myDamage"), 1050)
        parasites += scalar_field(name_hash("myBlastRadius"), 35)
        support = struct.pack("<III", 0x66666666, name_hash(support_name), 4)
        support += scalar_field(FIELD_MY_NUMBER_OF_PROJECTILES, 10)
        support += scalar_field(FIELD_MY_PROJECTILE_TYPE, "STRAIGHT")
        support += projectile + parasites
        group = struct.pack("<III", 0x77777777, 0x88888888, 1) + support
        root = struct.pack("<III", 0x99999999, 0xAAAAAAAA, 1) + group

        definitions = support_projectile_definitions(
            ICE_SIGNATURE + struct.pack("<II", 1, 1) + root,
            (
                f"SupportWeaponDatabase.US.{support_name}.myGuiName\tTest Strike\r\n"
            ).encode(),
        )
        definition = definitions[name_hash(support_name)][0]

        self.assertEqual(definition.mover_kind, "STRAIGHT")
        self.assertEqual(definition.projectile_count, 10)
        self.assertEqual(definition.model_file, "effects/shell.mrb")
        self.assertEqual(definition.post_hit_time_to_live, 1.25)
        self.assertEqual(definition.hit_effect, "Support_Test")
        self.assertEqual(
            [effect.type_name for effect in definition.effects],
            ["PP_DirectDamage", "PP_BlastDamage"],
        )
        self.assertEqual(
            definition.effects[1].scalar_fields,
            ((name_hash("myDamage"), "1050"), (name_hash("myBlastRadius"), "35")),
        )

    def test_decodes_support_clouds_in_serialized_global_index_order(self):
        support_name = "TacticalNuke_Test"
        values = {
            FIELD_MY_TIME_TO_LIVE: 1,
            FIELD_MY_HEALTH_CHANGE: -150000,
            FIELD_MY_HEALTH_CHANGE_INTERVAL: 0.5,
            FIELD_MY_RADIUS: 80,
            FIELD_MY_INITIAL_LOGIC_DELAY: 0,
            FIELD_MY_AFFECT_FRIENDLY_FLAG: 1,
            FIELD_MY_AFFECT_ENEMY_FLAG: 1,
            FIELD_MY_AFFECTS_INFANTRY_FLAG: 1,
            FIELD_MY_AFFECTS_VEHICLE_FLAG: 1,
            FIELD_MY_AFFECTS_TANKS_FLAG: 1,
            FIELD_MY_AFFECTS_COPTERS_FLAG: 1,
            FIELD_MY_AFFECTS_MISC_FLAG: 1,
            FIELD_MY_AFFECTS_BUILDINGS_FLAG: 1,
            FIELD_MY_INFANTRY_DAMAGE_MULTIPLIER: 1,
            FIELD_MY_GROUND_DAMAGE_MULTIPLIER: 1,
            FIELD_MY_HEAVY_ARMOR_DAMAGE_MULTIPLIER: 1,
            FIELD_MY_AIR_DAMAGE_MULTIPLIER: 1,
            FIELD_MY_BUILDING_DAMAGE_MULTIPLIER: 1,
        }
        cloud = struct.pack(
            "<III",
            TYPE_CLOUD_DEFINITION,
            0x11223344,
            CLOUD_DEFINITION_FIELD_COUNT,
        )
        cloud += b"".join(scalar_field(key, value) for key, value in values.items())
        for index in range(CLOUD_DEFINITION_FIELD_COUNT - len(values)):
            cloud += scalar_field(0x80000000 + index, 0)
        support = struct.pack("<III", 0x20000001, name_hash(support_name), 1) + cloud
        group = struct.pack("<III", 0x20000002, 0x20000003, 1) + support
        root = struct.pack("<III", 0x20000004, 0x20000005, 1) + group
        definitions = support_cloud_types(
            ICE_SIGNATURE + struct.pack("<II", 1, 1) + root,
            (
                f"SupportWeaponDatabase.US.{support_name}.myGuiName\tTest Nuke\r\n"
            ).encode(),
        )

        self.assertEqual(len(definitions), 1)
        self.assertEqual(definitions[0].index, 0)
        self.assertEqual(definitions[0].support_name, support_name)
        self.assertEqual(definitions[0].health_change, -150000)
        self.assertEqual(definitions[0].radius, 80.0)
        self.assertTrue(definitions[0].affects_buildings)

    def test_decodes_complete_tree_and_named_unit_parasite_types(self):
        unit_name = "TestUnit"
        self_destruct = name_hash("SelfDestruct")
        root = struct.pack("<III", 0x10000001, 0x10000002, 1)
        root += struct.pack("<III", 0x20000001, name_hash(unit_name), 6)
        root += text_field(FIELD_MY_TYPE, "GROUND")
        root += text_field(FIELD_MY_UNIT_CATEGORY, "TANKS")
        root += text_field(FIELD_MY_HEALTH, "100")
        root += scalar_field(FIELD_MY_SPEED, "12.5")
        root += scalar_field(FIELD_MY_MAX_SPEED_MULTIPLIER, "1.5")
        root += struct.pack("<III", 0x30000001, FIELD_MY_PARASITES, 2)
        root += struct.pack("<III", self_destruct, 0x40000001, 0)
        root += struct.pack("<III", name_hash("Squad"), 0x40000002, 0)
        data = ICE_SIGNATURE + struct.pack("<II", 1, 1) + root

        roots = ice_tree_roots(data)
        definitions = unit_type_parasites(
            data,
            b"UnitTypes.TestUnit.myUIName\tTest Unit\r\n",
        )

        self.assertEqual(roots[0].end, len(data))
        self.assertEqual(definitions[name_hash(unit_name)].name, unit_name)
        self.assertEqual(definitions[name_hash(unit_name)].meta_type, 1)
        self.assertEqual(definitions[name_hash(unit_name)].meta_type_name, "GROUND")
        self.assertEqual(definitions[name_hash(unit_name)].unit_category, 2)
        self.assertEqual(definitions[name_hash(unit_name)].unit_category_name, "TANKS")
        self.assertEqual(definitions[name_hash(unit_name)].max_health, 100)
        self.assertEqual(definitions[name_hash(unit_name)].max_speed, 12.5)
        self.assertEqual(definitions[name_hash(unit_name)].max_speed_multiplier, 1.5)
        self.assertEqual(
            definitions[name_hash(unit_name)].parasite_type_hashes,
            frozenset((self_destruct, name_hash("Squad"))),
        )

    def test_decodes_bounded_prop_instance_identity_type_and_position(self):
        instance_name = "bridgeFarmland__0"
        record = struct.pack(
            "<III",
            TYPE_WICED_PROP_INSTANCE,
            name_hash(instance_name),
            11,
        )
        record += text_field(FIELD_MY_NAME, instance_name)
        record += text_field(FIELD_MY_TYPE, "bridgeFarmland")
        record += vector3_field(FIELD_MY_POSITION, (1.25, 2.5, 3.75))
        record += vector3_field(FIELD_MY_HPB, (45.0, 0.0, 0.0))

        instance = prop_instances(ICE_SIGNATURE + record)[0]
        self.assertEqual(instance.position, (1.25, 2.5, 3.75))
        self.assertEqual(instance.hpb_degrees, (45.0, 0.0, 0.0))

    def test_reproduces_axis_aligned_bridge_fatal_bounds(self):
        element = struct.pack("<III", 0x10000001, 0x10000002, 4)
        element += scalar_field(FIELD_MY_TYPE, 0)
        element += vector3_field(name_hash("myPos"), (1.0, 0.0, -2.0))
        element += vector3_field(name_hash("myRotHPB"), (0.0, 0.0, 0.0))
        element += vector3_field(name_hash("myExtents"), (2.0, 1.0, 4.0))
        body = struct.pack("<III", 0x20000001, 0x20000002, 2)
        body += vector3_field(name_hash("myRootPosition"), (3.0, 0.0, 5.0))
        body += struct.pack("<III", 0x30000001, 0x30000002, 1) + element
        root = struct.pack("<III", 0x40000001, 0x40000002, 1) + body
        physics = ICE_SIGNATURE + struct.pack("<II", 0, 1) + root

        self.assertEqual(
            bridge_kill_bounds_xz(
                physics,
                position=(100.0, 20.0, 200.0),
                hpb_degrees=(0.0, 0.0, 0.0),
            ),
            (102.0, 199.0, 106.0, 207.0),
        )

    def test_bridge_types_require_a_nonempty_repair_link_list(self):
        ordinary = struct.pack("<III", TYPE_PROP_DEFINITION, 0x11111111, 40)
        ordinary += struct.pack("<III", 0x99999999, FIELD_MY_REPAIR_LINKS, 0xFFFFFFFF)
        bridge = struct.pack("<III", TYPE_PROP_DEFINITION, 0x22222222, 40)
        bridge += struct.pack("<III", 0x99999999, FIELD_MY_REPAIR_LINKS, 3)

        self.assertEqual(
            bridge_type_hashes(ICE_SIGNATURE + ordinary + bridge),
            {0x22222222},
        )


if __name__ == "__main__":
    unittest.main()
