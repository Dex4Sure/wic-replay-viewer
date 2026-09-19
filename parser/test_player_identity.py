import struct
import unittest

from wic_replay_parser import (
    WICReplayParserV4,
    Player,
    _BINTAG_SEP,
    _ENVELOPE_ARRAY_FLAG,
    _ENVELOPE_TYPE,
    _HASH_ADATA_TYPE,
    _HASH_AFLOAT,
    _HASH_APOS,
    _HASH_AROLE_ID,
    _HASH_ASLOT,
    _HASH_ASPECTATOR_LOS,
    _HASH_APLAYER,
    _HASH_PLAYER_SET_ROLE,
    _HASH_SPECTATOR_JOINED_TEAM,
    _HASH_UNIT_CREATE,
    _HASH_ATEAM,
    _HASH_EVENT,
    _HASH_PLAYER_ENTERS_GAME,
    _HASH_PLAYER_ENTRY_NAME,
    _HASH_PLAYER_LEAVES_GAME,
    _HASH_PLAYER_JOINED_TEAM,
    _HASH_SCORE,
    _HASH_SCORE_ROLE0,
    _HASH_SET_GAME_MODE_DATA_FLOAT,
    _HASH_TEAM_WINS,
)


def u32_field(field_hash: bytes, value: int) -> bytes:
    return field_hash + _BINTAG_SEP + b"\0" + _BINTAG_SEP + struct.pack("<I", value)


def float_field(field_hash: bytes, value: float) -> bytes:
    return field_hash + _BINTAG_SEP + b"\0" + _BINTAG_SEP + struct.pack("<f", value)


def utf16_field(field_hash: bytes, value: str) -> bytes:
    payload = value.encode("utf-16le") + b"\0\0"
    total = 13 + len(payload)
    return (
        field_hash
        + struct.pack("<I", total)
        + b"\x05"
        + struct.pack("<I", total)
        + payload
    )


def envelope(message: bytes, seconds: float, body: bytes) -> bytes:
    total = 17 + 4 + len(body)
    return (
        _HASH_EVENT
        + _ENVELOPE_TYPE
        + bytes([_ENVELOPE_ARRAY_FLAG])
        + struct.pack("<I", total)
        + struct.pack("<f", seconds)
        + message
        + body
    )


def player_entry(slot: int, name: str, seconds: float) -> bytes:
    body = b"".join(
        [
            u32_field(_HASH_ASLOT, slot),
            u32_field(b"\xa8\x01\x32\x04", 0),
            utf16_field(_HASH_PLAYER_ENTRY_NAME, name),
        ]
    )
    return envelope(_HASH_PLAYER_ENTERS_GAME, seconds, body)


def player_leave(slot: int, seconds: float) -> bytes:
    return envelope(_HASH_PLAYER_LEAVES_GAME, seconds, u32_field(_HASH_ASLOT, slot))


def gameplay_clock(seconds: float) -> bytes:
    body = u32_field(_HASH_ADATA_TYPE, 1) + float_field(_HASH_AFLOAT, 1200.0)
    return envelope(_HASH_SET_GAME_MODE_DATA_FLOAT, seconds, body)


def match_end(seconds: float) -> bytes:
    return envelope(_HASH_TEAM_WINS, seconds, b"")


def resolve(full_data: bytes, names=None, scored_slots=None):
    parser = WICReplayParserV4("fixture.wicdemo")
    parser.metadata_chunk = b""
    parser.full_data = full_data
    return parser._resolve_match_roster_names(
        names or {1: "Original"}, scored_slots or {1}
    )


class MatchRosterIdentityTests(unittest.TestCase):
    def test_departures_are_session_local_and_do_not_mark_reconnects(self):
        for case in (
            "left",
            "active",
            "reconnected",
            "replacement",
            "implicit replacement",
            "unknown replacement",
            "post result",
            "no entry",
            "late capture",
            "no result",
        ):
            with self.subTest(case=case):
                data = (
                    b""
                    if case == "late capture"
                    else envelope(
                        _HASH_SET_GAME_MODE_DATA_FLOAT,
                        0.0,
                        u32_field(_HASH_ADATA_TYPE, 1) + float_field(_HASH_AFLOAT, 0.0),
                    )
                )
                if case != "no entry":
                    data += player_entry(1, "Original", 0.1)
                data += gameplay_clock(1.0) + gameplay_clock(1.1) + gameplay_clock(1.2)
                if case not in ("active", "post result", "implicit replacement"):
                    data += player_leave(1, 2.0)
                if case in (
                    "reconnected",
                    "replacement",
                    "implicit replacement",
                    "unknown replacement",
                ):
                    data += player_entry(
                        1,
                        "Original"
                        if case == "reconnected"
                        else ""
                        if case == "unknown replacement"
                        else "Other",
                        3.0,
                    )
                if case != "no result":
                    data += match_end(4.0)
                if case == "post result":
                    data += player_leave(1, 5.0)
                parser = WICReplayParserV4("synthetic.wicdemo")
                parser.full_data = data
                rows = [Player(id=1, name="Original", score=115)]
                parser._mark_result_departures(rows, {1: "Original"})
                self.assertEqual(
                    rows[0].left_at_seconds,
                    2.0 if case in ("left", "replacement", "late capture") else None,
                )
                self.assertEqual(rows[0].score, 115)

    def test_nonbreaking_spaces_in_names_match_the_native_decoder(self):
        for name in ("\xa0^-sheepy-", "ƒail^\xa0Vikoz"):
            self.assertEqual(
                WICReplayParserV4._read_utf16le_name(
                    (name + "\0").encode("utf-16le"), 0
                ),
                name,
            )
            self.assertEqual(
                WICReplayParserV4._read_bintag_utf16_string(
                    utf16_field(_HASH_PLAYER_ENTRY_NAME, name),
                    0,
                    _HASH_PLAYER_ENTRY_NAME,
                    trim_trailing_nbsp=True,
                ),
                name,
            )
        self.assertIsNone(
            WICReplayParserV4._read_utf16le_name("bad\tname\0".encode("utf-16le"), 0)
        )

    def test_zero_score_spectator_correction_requires_session_evidence(self):
        for case in (
            "spectator",
            "one team spectator",
            "invalid spectator view",
            "lobby role explicit late spectator",
            "lobby role spectator",
            "lobby role late spectator",
            "lobby role rejoin",
            "lobby role after spectator",
            "lobby role match role",
            "lobby units spectator",
            "replacement",
            "positive score",
            "summary score",
            "units",
            "role selection",
            "still playing team",
            "postmatch spectator",
            "missing spectator",
            "missing result",
            "late recording",
            "missing score",
            "missing summary",
            "reconnect",
            "unframed spectator",
            "unknown team",
        ):
            with self.subTest(case=case):

                def join(seconds):
                    return envelope(
                        _HASH_PLAYER_JOINED_TEAM,
                        seconds,
                        u32_field(_HASH_ASLOT, 1)
                        + u32_field(_HASH_ATEAM, 42 if case == "unknown team" else 3),
                    )

                def spectate(seconds):
                    return envelope(
                        _HASH_SPECTATOR_JOINED_TEAM,
                        seconds,
                        u32_field(_HASH_ASLOT, 1)
                        + u32_field(
                            _HASH_ATEAM, 3 if case == "one team spectator" else 0
                        )
                        + u32_field(
                            _HASH_ASPECTATOR_LOS,
                            1
                            if case == "one team spectator"
                            else 99
                            if case == "invalid spectator view"
                            else 2,
                        ),
                    )

                data = b""
                if case != "late recording":
                    data += envelope(
                        _HASH_SET_GAME_MODE_DATA_FLOAT,
                        0.0,
                        u32_field(_HASH_ADATA_TYPE, 1) + float_field(_HASH_AFLOAT, 0.0),
                    )
                data += join(0.1)
                if case == "lobby role explicit late spectator":
                    data += player_entry(1, "Original", 0.15)
                if case.startswith("lobby"):
                    if case == "lobby role after spectator":
                        data += spectate(0.2)
                    data += envelope(
                        _HASH_PLAYER_SET_ROLE,
                        0.3,
                        u32_field(_HASH_ASLOT, 1)
                        + u32_field(_HASH_AROLE_ID, 0x10E00313),
                    )
                    if case == "lobby units spectator":
                        data += envelope(
                            _HASH_UNIT_CREATE,
                            0.4,
                            u32_field(bytes(4), 123)
                            + u32_field(_HASH_APLAYER, 1)
                            + u32_field(_HASH_ATEAM, 3),
                        )
                    if case not in (
                        "lobby role late spectator",
                        "lobby role explicit late spectator",
                        "lobby role after spectator",
                    ):
                        data += spectate(0.5)
                data += gameplay_clock(1.0)
                if case == "lobby role rejoin":
                    data += join(1.05)
                data += gameplay_clock(1.1) + gameplay_clock(1.2)
                if case == "units":
                    data += envelope(
                        _HASH_UNIT_CREATE,
                        1.3,
                        u32_field(bytes(4), 123)
                        + u32_field(_HASH_APLAYER, 1)
                        + u32_field(_HASH_ATEAM, 3),
                    )
                if case in ("role selection", "lobby role match role"):
                    data += envelope(
                        _HASH_PLAYER_SET_ROLE,
                        1.3,
                        u32_field(_HASH_ASLOT, 1)
                        + u32_field(_HASH_AROLE_ID, 0x10E00313),
                    )
                if case not in (
                    "missing spectator",
                    "postmatch spectator",
                    "unframed spectator",
                ):
                    data += spectate(2.0)
                if case in ("replacement", "reconnect"):
                    data += player_leave(1, 2.1)
                    data += player_entry(
                        1, "Other" if case == "replacement" else "Original", 2.2
                    )
                    data += join(2.3) + spectate(2.4)
                if case == "still playing team":
                    data += join(2.5)
                if case != "missing result":
                    data += match_end(3.0)
                if case == "postmatch spectator":
                    data += spectate(3.1)
                if case == "unframed spectator":
                    data += _HASH_SPECTATOR_JOINED_TEAM + u32_field(_HASH_ASLOT, 1)
                    data += u32_field(_HASH_ATEAM, 0) + u32_field(
                        _HASH_ASPECTATOR_LOS, 2
                    )
                score = 100 if case == "positive score" else 0
                if case != "missing score":
                    data += u32_field(_HASH_SCORE, score).ljust(47, b"\0")
                    data += _BINTAG_SEP + struct.pack("<I", 2)
                if case != "missing summary":
                    summary = u32_field(_HASH_APOS, 1) + u32_field(
                        _HASH_AROLE_ID, 0x10E00313
                    )
                    summary += u32_field(
                        _HASH_SCORE_ROLE0, 1 if case == "summary score" else 0
                    )
                    data += summary.ljust(300, b"\0")
                parser = WICReplayParserV4("fixture.wicdemo")
                parser.metadata_chunk = (
                    u32_field(_HASH_ASLOT, 1)
                    + bytes(30)
                    + "Original\0".encode("utf-16le")
                )
                parser.full_data = data
                # The Python CLI rejects missing-result files before projection.
                if case == "missing result":
                    with self.assertRaises(ValueError):
                        parser.parse()
                    continue
                player = next(p for p in parser.parse().players if p.id == 1)
                self.assertEqual(player.name, "Original")
                self.assertEqual(
                    player.score, None if case == "missing score" else score
                )
                expected = (
                    0
                    if case
                    in (
                        "spectator",
                        "replacement",
                        "lobby role spectator",
                        "one team spectator",
                        "lobby role explicit late spectator",
                    )
                    else 42
                    if case == "unknown team"
                    else 3
                )
                self.assertEqual(player.team, expected)

    def test_recorded_role_fills_only_scored_players_without_role_totals(self):
        for role_id, role_score, score, expected in (
            (0x1ABF03DF, 0, 930, "support"),
            (0x1ABF03DF, 12, 930, "infantry"),
            (0x1ABF03DF, 0, 0, None),
            (0x0B230275, 0, 930, None),
            (42, 0, 930, None),
        ):
            with self.subTest(role_id=role_id, role_score=role_score, score=score):
                data = u32_field(_HASH_SCORE, score).ljust(47, b"\0")
                data += _BINTAG_SEP + struct.pack("<I", 9)
                data += u32_field(_HASH_APOS, 8)
                data += u32_field(_HASH_AROLE_ID, role_id)
                data += u32_field(_HASH_SCORE_ROLE0, role_score)
                data = data.ljust(355, b"\0") + match_end(1200.0)
                parser = WICReplayParserV4("fixture.wicdemo")
                parser.metadata_chunk = (
                    u32_field(_HASH_ASLOT, 8)
                    + bytes(30)
                    + "Small Island\0".encode("utf-16le")
                )
                parser.full_data = data
                player = next(p for p in parser.parse().players if p.id == 8)
                self.assertEqual(player.role, expected)
                self.assertEqual(player.score, score)
                self.assertEqual(player.score_infantry, role_score)
                self.assertEqual(player.score_support, 0)

    def test_late_lobby_correction_requires_one_entrant_with_known_role_and_team(self):
        for case in (
            "sole entrant",
            "negative role",
            "two entrants",
            "missing role",
            "missing result",
            "team mismatch",
        ):
            with self.subTest(case=case):
                data = player_leave(1, 0.2) + gameplay_clock(1.0)
                data += player_entry(1, "Replacement", 2.0)
                data += envelope(
                    _HASH_PLAYER_JOINED_TEAM,
                    2.1,
                    u32_field(_HASH_ASLOT, 1) + u32_field(_HASH_ATEAM, 3),
                )
                if case == "two entrants":
                    data += player_entry(1, "Second", 2.5)
                if case != "missing result":
                    data += match_end(3.0)
                if case == "team mismatch":
                    data += envelope(
                        _HASH_PLAYER_JOINED_TEAM,
                        3.5,
                        u32_field(_HASH_ASLOT, 1) + u32_field(_HASH_ATEAM, 2),
                    )
                summary = u32_field(_HASH_APOS, 1)
                if case != "missing role":
                    summary += u32_field(
                        _HASH_SCORE_ROLE0,
                        2**32 - 12 if case == "negative role" else 247,
                    )
                data += envelope(b"\x09" * 4, 4.0, summary.ljust(300, b"\0"))
                self.assertEqual(
                    resolve(data),
                    {
                        1: "Replacement"
                        if case in ("sole entrant", "negative role")
                        else "Original"
                    },
                )

    def test_replacement_before_gameplay_supersedes_static_metadata(self):
        full_data = (
            player_leave(1, 0.2)
            + player_entry(1, "Replacement\xa0\xa0", 0.3)
            + gameplay_clock(1.0)
            + match_end(2.0)
        )

        self.assertEqual(resolve(full_data), {1: "Replacement"})

    def test_replacement_after_gameplay_does_not_relabel_scored_roster(self):
        full_data = (
            gameplay_clock(1.0)
            + player_leave(1, 2.0)
            + player_entry(1, "Replacement", 3.0)
            + match_end(4.0)
        )

        self.assertEqual(resolve(full_data), {1: "Original"})

    def test_vacant_scored_slot_at_gameplay_start_preserves_static_name(self):
        full_data = player_leave(1, 0.2) + gameplay_clock(1.0) + match_end(2.0)

        self.assertEqual(resolve(full_data), {1: "Original"})

    def test_untracked_entry_does_not_expand_the_static_roster(self):
        full_data = (
            player_entry(2, "Untracked", 0.3) + gameplay_clock(1.0) + match_end(2.0)
        )

        self.assertEqual(resolve(full_data), {1: "Original"})

    def test_missing_gameplay_clock_preserves_static_metadata(self):
        full_data = player_leave(1, 0.2) + player_entry(1, "Replacement", 0.3)

        self.assertEqual(resolve(full_data), {1: "Original"})


if __name__ == "__main__":
    unittest.main()
