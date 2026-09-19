import struct
import unittest
from types import SimpleNamespace

from ta_projectile_simulation import (
    SUPPORT_FAMILIES,
    WIRE_SHAPES,
    decode_support_projectile,
    simulate_ballistic_free_flight,
    simulate_straight_free_flight,
)
from wic_bintag import FIELD_SEP, TYPE_FLOAT32


def encoded_field(field_hash, flag, value):
    raw = struct.pack("<f" if flag == TYPE_FLOAT32 else "<I", value)
    return struct.pack("<I", field_hash) + FIELD_SEP + bytes((flag,)) + FIELD_SEP + raw


def projectile_envelope(family, values, trailing=b""):
    body = b"".join(
        encoded_field(field_hash, flag, value)
        for (field_hash, flag), value in zip(WIRE_SHAPES[family], values, strict=True)
    )
    message = next(key for key, value in SUPPORT_FAMILIES.items() if value == family)
    data = body + trailing
    envelope = SimpleNamespace(
        message=message,
        body_start=0,
        body_end=len(data),
        offset=0x1234,
        time=12.5,
    )
    return data, envelope


class ProjectileSimulationTests(unittest.TestCase):
    def test_decodes_all_four_complete_support_wire_families(self):
        rows = {
            "straight": [10, 20, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 2],
            "ballistic": [11, 21, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 3],
            "homingPosition": [
                12,
                22,
                1.0,
                2.0,
                3.0,
                4.0,
                5.0,
                6.0,
                0.75,
                7.0,
                8.0,
                9.0,
                4,
            ],
            "homingUnit": [
                13,
                23,
                1.0,
                2.0,
                3.0,
                4.0,
                5.0,
                6.0,
                0.5,
                99,
                5,
            ],
        }
        decoded = {}
        for family, values in rows.items():
            data, envelope = projectile_envelope(family, values)
            decoded[family] = decode_support_projectile(data, envelope)

        self.assertEqual(decoded["straight"].lifecycle_key, (10, 0x1234))
        self.assertEqual(decoded["ballistic"].source, (1.0, 2.0, 3.0))
        self.assertEqual(decoded["homingPosition"].target_position, (7.0, 8.0, 9.0))
        self.assertEqual(decoded["homingUnit"].target_unit_id, 99)
        self.assertEqual(decoded["homingUnit"].track_factor, 0.5)

    def test_projectile_id_reuse_is_disambiguated_by_creation_offset(self):
        values = [10, 20, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 2]
        data, first_envelope = projectile_envelope("straight", values)
        first = decode_support_projectile(data, first_envelope)
        second_envelope = SimpleNamespace(**vars(first_envelope))
        second_envelope.offset = 0x5678
        second = decode_support_projectile(data, second_envelope)

        self.assertNotEqual(first.lifecycle_key, second.lifecycle_key)
        self.assertEqual(first.projectile_id, second.projectile_id)

    def test_rejects_trailing_or_partial_wire_data(self):
        values = [10, 20, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 2]
        data, envelope = projectile_envelope("straight", values, trailing=b"x")
        with self.assertRaisesRegex(ValueError, "wire mismatch"):
            decode_support_projectile(data, envelope)

    def test_straight_and_ballistic_free_flight_use_supplied_tick_sequence(self):
        straight_position, straight_velocity = simulate_straight_free_flight(
            (1.0, 2.0, 3.0), (10.0, 0.0, -2.0), (0.5,)
        )
        self.assertEqual(straight_position, (6.0, 2.0, 2.0))
        self.assertEqual(straight_velocity, (10.0, 0.0, -2.0))

        ballistic_position, ballistic_velocity = simulate_ballistic_free_flight(
            (0.0, 10.0, 0.0), (1.0, 4.0, 0.0), (0.5,)
        )
        self.assertEqual(ballistic_position, (0.5, 12.0, 0.0))
        self.assertAlmostEqual(ballistic_velocity[1], -0.905, places=5)


if __name__ == "__main__":
    unittest.main()
