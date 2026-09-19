"""Read-only evidence extraction for replay timeline player attribution.

This module is intentionally a research layer. It exposes exact marker player slots,
raw tactical-aid facts, and conservative purchase-to-spawn candidates without
changing the canonical replay parser or promoting candidates into the public
timeline schema.
"""

from __future__ import annotations

import hashlib
import math
import pathlib
from collections import Counter, defaultdict
from dataclasses import dataclass

from wic_bintag import decompress, fields, name_hash, recorder_slot, walk

MSG_CHANGE_HONORS = name_hash("ChangeHonors")
MSG_SUPPORT_THING_USED = name_hash("SupportThingUsed")
MSG_SUPPORT_THING_FEEDBACK = name_hash("SupportThingFeedback")
MSG_SUPPORT_THING_SPAWNED_DELAYED = name_hash("SupportThingSpawnedDelayed")
MSG_SUPPORT_THING_MARKER = name_hash("SupportThingMarker")
MSG_SUPPORT_THING_MARKER_STOPPED = name_hash("SupportThingMarkerStopped")
MSG_PROJECTILE_STRAIGHT_SUPPORT_CREATE = name_hash("ProjectileStraightSupportCreate")
MSG_PROJECTILE_BALLISTIC_SUPPORT_CREATE = name_hash("ProjectileBallisticSupportCreate")
MSG_PROJECTILE_HOMING_SUPPORT_POSITION_CREATE = name_hash(
    "ProjectileHomingSupportCreate_Position"
)
MSG_PROJECTILE_HOMING_SUPPORT_UNIT_CREATE = name_hash(
    "ProjectileHomingSupportCreate_Unit"
)
MSG_REQUEST_SENT = name_hash("RequestSent")

POSITION_EPSILON_DEFAULT = 0.0


def _rounded(value: float) -> float:
    return round(value, 6)


def _position(field_values, start: int) -> tuple[float, float, float]:
    return tuple(field.f32 for field in field_values[start : start + 3])


def _position_json(position: tuple[float, float, float]) -> list[float]:
    return [_rounded(value) for value in position]


def _support_id_json(value: int) -> str:
    return f"0x{value:08x}"


def _sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


@dataclass(frozen=True)
class SupportPurchase:
    event_index: int
    offset: int
    raw_event_time: float
    support_type_id: int
    position: tuple[float, float, float]
    recorder_player_id: int | None
    honors_delta: float | None


@dataclass(frozen=True)
class SupportSpawn:
    event_index: int
    offset: int
    raw_event_time: float
    support_type_id: int
    position: tuple[float, float, float]
    team: int
    upgrade_level: int
    direction: tuple[float, float, float]
    age_seconds: float


@dataclass(frozen=True)
class SupportMarker:
    event_index: int
    offset: int
    raw_event_time: float
    event_id: int
    support_type_id: int
    position: tuple[float, float, float]
    player_id: int
    upgrade_level: int
    direction: tuple[float, float, float]
    duration_seconds: float


@dataclass(frozen=True)
class SupportMarkerStop:
    event_index: int
    offset: int
    raw_event_time: float
    event_id: int


def position_distance(
    left: tuple[float, float, float], right: tuple[float, float, float]
) -> float:
    """Return Euclidean distance between two serialized world positions."""
    return math.dist(left, right)


def _unique_component_matching(
    purchase_indices: set[int],
    purchase_candidates: dict[int, list[tuple[int, float]]],
    effects: list[SupportSpawn] | list[SupportMarker],
) -> dict[int, int] | None:
    """Return the sole complete one-to-one assignment, or None.

    Search stops after finding a second solution. Candidate components are small in
    practice, and choosing the most constrained remaining purchase first keeps the
    duplicate-stream cases bounded without introducing a scoring heuristic.
    """
    solutions: list[dict[int, int]] = []

    def search(assignment: dict[int, int], used_effects: set[int]) -> None:
        if len(solutions) >= 2:
            return
        if len(assignment) == len(purchase_indices):
            solutions.append(dict(assignment))
            return

        remaining = purchase_indices - assignment.keys()
        available_by_purchase = {
            purchase_index: [
                effect_index
                for effect_index, _ in purchase_candidates[purchase_index]
                if effect_index not in used_effects
            ]
            for purchase_index in remaining
        }
        purchase_index = min(
            remaining,
            key=lambda index: (len(available_by_purchase[index]), index),
        )
        available = available_by_purchase[purchase_index]
        if not available:
            return
        for effect_index in sorted(
            available, key=lambda index: (effects[index].offset, index)
        ):
            assignment[purchase_index] = effect_index
            used_effects.add(effect_index)
            search(assignment, used_effects)
            used_effects.remove(effect_index)
            del assignment[purchase_index]
            if len(solutions) >= 2:
                return

    search({}, set())
    return solutions[0] if len(solutions) == 1 else None


def _match_purchase_effect_candidates(
    purchases: list[SupportPurchase],
    effects: list[SupportSpawn] | list[SupportMarker],
    position_epsilon: float,
    effect_kind: str,
) -> list[dict]:
    """Build conservative purchase-to-effect candidates.

    A candidate must share the support type, occur no earlier in stream order, and
    have a position within the configured epsilon. No elapsed-time cutoff is used.
    A proposed link is unique only when its complete connected candidate component
    has exactly one one-to-one matching under those rules.
    """
    purchase_candidates: dict[int, list[tuple[int, float]]] = defaultdict(list)
    effect_candidates: dict[int, list[tuple[int, float]]] = defaultdict(list)

    for purchase_index, purchase in enumerate(purchases):
        for effect_index, effect in enumerate(effects):
            if effect.support_type_id != purchase.support_type_id:
                continue
            if effect.offset < purchase.offset:
                continue
            distance = position_distance(purchase.position, effect.position)
            if distance > position_epsilon:
                continue
            purchase_candidates[purchase_index].append((effect_index, distance))
            effect_candidates[effect_index].append((purchase_index, distance))

    unique_assignments: dict[int, int] = {}
    ambiguous_purchases: set[int] = set()
    remaining_purchases = set(purchase_candidates)
    while remaining_purchases:
        root = min(remaining_purchases)
        component_purchases: set[int] = set()
        component_effects: set[int] = set()
        pending_purchases = [root]
        while pending_purchases:
            purchase_index = pending_purchases.pop()
            if purchase_index in component_purchases:
                continue
            component_purchases.add(purchase_index)
            for effect_index, _ in purchase_candidates[purchase_index]:
                if effect_index in component_effects:
                    continue
                component_effects.add(effect_index)
                pending_purchases.extend(
                    other_purchase
                    for other_purchase, _ in effect_candidates[effect_index]
                    if other_purchase not in component_purchases
                )
        remaining_purchases -= component_purchases
        matching = _unique_component_matching(
            component_purchases, purchase_candidates, effects
        )
        if matching is None:
            ambiguous_purchases.update(component_purchases)
        else:
            unique_assignments.update(matching)

    results = []
    for purchase_index, purchase in enumerate(purchases):
        candidates = purchase_candidates[purchase_index]
        selected_effect = unique_assignments.get(purchase_index)
        if selected_effect is not None:
            status = "unique"
        elif purchase_index in ambiguous_purchases:
            status = "ambiguous"
        else:
            status = "unmatched"

        candidate_rows = []
        selected_candidate_index = None
        for effect_index, distance in candidates:
            effect = effects[effect_index]
            candidate = {
                "effectKind": effect_kind,
                "effectOffset": effect.offset,
                "effectRawEventTime": _rounded(effect.raw_event_time),
                "positionDistance": _rounded(distance),
                "rawTimeDelta": _rounded(
                    effect.raw_event_time - purchase.raw_event_time
                ),
                "reverseCandidateCount": len(effect_candidates[effect_index]),
            }
            if isinstance(effect, SupportMarker):
                candidate["effectEventId"] = effect.event_id
                candidate["effectPlayerId"] = effect.player_id
            else:
                candidate["effectTeam"] = effect.team
            candidate["selected"] = effect_index == selected_effect
            if candidate["selected"]:
                selected_candidate_index = len(candidate_rows)
            candidate_rows.append(candidate)

        result = {
            "purchaseOffset": purchase.offset,
            "purchaseRawEventTime": _rounded(purchase.raw_event_time),
            "supportTypeId": _support_id_json(purchase.support_type_id),
            "position": _position_json(purchase.position),
            "recorderPlayerId": purchase.recorder_player_id,
            "status": status,
            "candidateBasis": [
                "sameSupportType",
                "positionWithinEpsilon",
                "nonnegativeStreamOrder",
            ],
            "candidates": candidate_rows,
        }
        if selected_candidate_index is not None:
            result["selectedCandidateIndex"] = selected_candidate_index
            result["selectionBasis"] = "uniqueGlobalOneToOneMatching"
        if (
            selected_effect is not None
            and purchase.recorder_player_id is not None
            and purchase.honors_delta is not None
            and purchase.honors_delta < 0
        ):
            if effect_kind == "marker":
                result["attributionCrossCheck"] = {
                    "recorderPlayerId": purchase.recorder_player_id,
                    "markerPlayerId": effects[selected_effect].player_id,
                    "agrees": (
                        purchase.recorder_player_id
                        == effects[selected_effect].player_id
                    ),
                    "basis": "independentRecorderPurchaseLedger",
                }
            else:
                result["proposedAttribution"] = {
                    "playerId": purchase.recorder_player_id,
                    "proposedClass": "derivedUnique",
                    "validationState": "researchCandidate",
                    "basis": (
                        f"recorderPurchaseTo{effect_kind[0].upper()}{effect_kind[1:]}"
                    ),
                }
        results.append(result)

    return results


def match_purchase_spawn_candidates(
    purchases: list[SupportPurchase],
    spawns: list[SupportSpawn],
    position_epsilon: float,
) -> list[dict]:
    return _match_purchase_effect_candidates(
        purchases, spawns, position_epsilon, "spawnDelayed"
    )


def match_purchase_marker_candidates(
    purchases: list[SupportPurchase],
    markers: list[SupportMarker],
    position_epsilon: float,
) -> list[dict]:
    return _match_purchase_effect_candidates(
        purchases, markers, position_epsilon, "marker"
    )


def _purchase_json(purchase: SupportPurchase) -> dict:
    paired_ledger = purchase.honors_delta is not None and purchase.honors_delta < 0
    exact_player = paired_ledger and purchase.recorder_player_id is not None
    return {
        "eventIndex": purchase.event_index,
        "offset": purchase.offset,
        "rawEventTime": _rounded(purchase.raw_event_time),
        "supportTypeId": _support_id_json(purchase.support_type_id),
        "position": _position_json(purchase.position),
        "honorsDelta": (
            _rounded(purchase.honors_delta)
            if purchase.honors_delta is not None
            else None
        ),
        "attribution": {
            "class": "exact" if exact_player else "unknown",
            "playerId": purchase.recorder_player_id if exact_player else None,
            "basis": "recorderHonorsLedger" if exact_player else None,
        },
    }


def _spawn_json(spawn: SupportSpawn) -> dict:
    return {
        "eventIndex": spawn.event_index,
        "offset": spawn.offset,
        "rawEventTime": _rounded(spawn.raw_event_time),
        "supportTypeId": _support_id_json(spawn.support_type_id),
        "position": _position_json(spawn.position),
        "team": spawn.team,
        "upgradeLevel": spawn.upgrade_level,
        "direction": _position_json(spawn.direction),
        "ageSeconds": _rounded(spawn.age_seconds),
        "attribution": {
            "class": "teamOnly",
            "playerId": None,
            "basis": "serializedSupportEffectTeam",
        },
    }


def _marker_json(marker: SupportMarker) -> dict:
    return {
        "eventIndex": marker.event_index,
        "offset": marker.offset,
        "rawEventTime": _rounded(marker.raw_event_time),
        "eventId": marker.event_id,
        "supportTypeId": _support_id_json(marker.support_type_id),
        "position": _position_json(marker.position),
        "playerId": marker.player_id,
        "serializedFieldName": "aTeam",
        "upgradeLevel": marker.upgrade_level,
        "direction": _position_json(marker.direction),
        "durationSeconds": _rounded(marker.duration_seconds),
        "attribution": {
            "class": "exact",
            "playerId": marker.player_id,
            "basis": "serializedSupportMarkerPlayerSlot",
        },
    }


def marker_support_type_stats(markers: list[SupportMarker]) -> dict[str, dict]:
    """Return compact, deterministic fingerprints for every deployed marker ID."""
    grouped: dict[int, list[SupportMarker]] = defaultdict(list)
    for marker in markers:
        grouped[marker.support_type_id].append(marker)

    result = {}
    for support_type_id, items in sorted(grouped.items()):
        players = Counter(item.player_id for item in items)
        upgrades = Counter(item.upgrade_level for item in items)
        durations = [item.duration_seconds for item in items]
        zero_direction = sum(item.direction == (0.0, 0.0, 0.0) for item in items)
        result[_support_id_json(support_type_id)] = {
            "deployments": len(items),
            "playerSlotCounts": {
                str(player): count for player, count in sorted(players.items())
            },
            "upgradeLevelCounts": {
                str(level): count for level, count in sorted(upgrades.items())
            },
            "durationSecondsRange": {
                "minimum": _rounded(min(durations)),
                "maximum": _rounded(max(durations)),
            },
            "zeroDuration": sum(duration == 0.0 for duration in durations),
            "zeroDirection": zero_direction,
            "nonzeroDirection": len(items) - zero_direction,
            "worldOriginPosition": sum(
                item.position == (0.0, 0.0, 0.0) for item in items
            ),
        }
    return result


def link_marker_stops(
    markers: list[SupportMarker], stops: list[SupportMarkerStop]
) -> list[dict]:
    """Link marker-stop records to the active marker with the same event ID.

    The client keeps one active marker per event ID and removes it when the stop
    message arrives. Preserve ambiguous or unmatched states if damaged input
    violates that lifecycle instead of guessing which start was intended.
    """
    lifecycle = [("start", marker) for marker in markers]
    lifecycle.extend(("stop", stop) for stop in stops)
    lifecycle.sort(key=lambda item: (item[1].offset, 0 if item[0] == "start" else 1))
    active: dict[int, list[SupportMarker]] = defaultdict(list)
    results = []
    for kind, item in lifecycle:
        if kind == "start":
            active[item.event_id].append(item)
            continue

        candidates = active.get(item.event_id, [])
        if len(candidates) == 1:
            marker = candidates[0]
            status = "exact"
            player_id = marker.player_id
            basis = "markerEventIdLifecycle"
            del active[item.event_id]
        elif candidates:
            marker = None
            status = "ambiguous"
            player_id = None
            basis = None
            del active[item.event_id]
        else:
            marker = None
            status = "unmatched"
            player_id = None
            basis = None

        result = {
            "eventIndex": item.event_index,
            "offset": item.offset,
            "rawEventTime": _rounded(item.raw_event_time),
            "eventId": item.event_id,
            "markerOffset": marker.offset if marker is not None else None,
            "attribution": {
                "class": status,
                "playerId": player_id,
                "basis": basis,
            },
        }
        results.append(result)
    return results


def analyse_replay(
    path: str | pathlib.Path,
    position_epsilon: float = POSITION_EPSILON_DEFAULT,
    include_event_rows: bool = False,
) -> dict:
    """Extract tactical-aid attribution evidence from one replay."""
    replay_path = pathlib.Path(path)
    data = decompress(replay_path)
    recorder = recorder_slot(data)
    purchases: list[SupportPurchase] = []
    spawns: list[SupportSpawn] = []
    markers: list[SupportMarker] = []
    marker_stops: list[SupportMarkerStop] = []
    feedback = []
    projectiles = []
    requests = []
    feedback_count = 0
    projectile_count = 0
    request_count = 0
    catalogue_count = 0
    malformed = Counter()
    envelope_count = 0
    covered_bytes = 0
    first_offset = None
    last_body_end = None

    previous = None
    for event_index, envelope in enumerate(walk(data)):
        envelope_count += 1
        covered_bytes += envelope.total
        if first_offset is None:
            first_offset = envelope.offset
        last_body_end = envelope.body_end

        if envelope.message == MSG_SUPPORT_THING_USED:
            body, _ = fields(data, envelope)
            if len(body) < 4:
                malformed["SupportThingUsed"] += 1
            else:
                position = _position(body, 1)
                if position == (0.0, 0.0, 0.0):
                    catalogue_count += 1
                else:
                    honors_delta = None
                    if previous is not None and previous.message == MSG_CHANGE_HONORS:
                        previous_body, _ = fields(data, previous)
                        if previous_body:
                            honors_delta = previous_body[0].f32
                    purchases.append(
                        SupportPurchase(
                            event_index=event_index,
                            offset=envelope.offset,
                            raw_event_time=envelope.time,
                            support_type_id=body[0].u32,
                            position=position,
                            recorder_player_id=recorder,
                            honors_delta=honors_delta,
                        )
                    )

        elif envelope.message == MSG_SUPPORT_THING_FEEDBACK:
            body, _ = fields(data, envelope)
            if len(body) < 2:
                malformed["SupportThingFeedback"] += 1
            else:
                feedback_count += 1
                if include_event_rows:
                    feedback.append(
                        {
                            "eventIndex": event_index,
                            "offset": envelope.offset,
                            "rawEventTime": _rounded(envelope.time),
                            "supportTypeId": _support_id_json(body[0].u32),
                            "team": body[1].i32,
                            "attribution": {
                                "class": "teamOnly",
                                "playerId": None,
                                "basis": "serializedSupportEffectTeam",
                            },
                        }
                    )

        elif envelope.message == MSG_SUPPORT_THING_SPAWNED_DELAYED:
            body, _ = fields(data, envelope)
            if len(body) < 10:
                malformed["SupportThingSpawnedDelayed"] += 1
            else:
                spawns.append(
                    SupportSpawn(
                        event_index=event_index,
                        offset=envelope.offset,
                        raw_event_time=envelope.time,
                        support_type_id=body[0].u32,
                        position=_position(body, 1),
                        team=body[4].i32,
                        upgrade_level=body[5].u32,
                        direction=_position(body, 6),
                        age_seconds=body[9].f32,
                    )
                )

        elif envelope.message == MSG_SUPPORT_THING_MARKER:
            body, _ = fields(data, envelope)
            if len(body) < 11:
                malformed["SupportThingMarker"] += 1
            else:
                markers.append(
                    SupportMarker(
                        event_index=event_index,
                        offset=envelope.offset,
                        raw_event_time=envelope.time,
                        event_id=body[0].u32,
                        support_type_id=body[1].u32,
                        position=_position(body, 2),
                        player_id=body[5].i32,
                        upgrade_level=body[6].u32,
                        direction=_position(body, 7),
                        duration_seconds=body[10].f32,
                    )
                )

        elif envelope.message == MSG_SUPPORT_THING_MARKER_STOPPED:
            body, _ = fields(data, envelope)
            if not body:
                malformed["SupportThingMarkerStopped"] += 1
            else:
                marker_stops.append(
                    SupportMarkerStop(
                        event_index=event_index,
                        offset=envelope.offset,
                        raw_event_time=envelope.time,
                        event_id=body[0].u32,
                    )
                )

        elif envelope.message in {
            MSG_PROJECTILE_STRAIGHT_SUPPORT_CREATE,
            MSG_PROJECTILE_BALLISTIC_SUPPORT_CREATE,
        }:
            body, _ = fields(data, envelope)
            if len(body) < 2:
                malformed["ProjectileSupportCreate"] += 1
            else:
                projectile_count += 1
                kind = (
                    "straight"
                    if envelope.message == MSG_PROJECTILE_STRAIGHT_SUPPORT_CREATE
                    else "ballistic"
                )
                if include_event_rows:
                    projectiles.append(
                        {
                            "eventIndex": event_index,
                            "offset": envelope.offset,
                            "rawEventTime": _rounded(envelope.time),
                            "kind": kind,
                            "projectileId": body[0].u32,
                            "supportTypeId": _support_id_json(body[1].u32),
                            "playerId": None,
                        }
                    )

        elif envelope.message in {
            MSG_PROJECTILE_HOMING_SUPPORT_POSITION_CREATE,
            MSG_PROJECTILE_HOMING_SUPPORT_UNIT_CREATE,
        }:
            body, _ = fields(data, envelope)
            if len(body) < 2:
                malformed["ProjectileHomingSupportCreate"] += 1
            else:
                projectile_count += 1
                kind = (
                    "homingPosition"
                    if envelope.message == MSG_PROJECTILE_HOMING_SUPPORT_POSITION_CREATE
                    else "homingUnit"
                )
                projectile = {
                    "eventIndex": event_index,
                    "offset": envelope.offset,
                    "rawEventTime": _rounded(envelope.time),
                    "kind": kind,
                    "projectileId": body[0].u32,
                    "supportTypeId": _support_id_json(body[1].u32),
                    "playerId": None,
                }
                if kind == "homingUnit" and len(body) >= 10:
                    projectile["targetUnitId"] = body[9].u32
                if include_event_rows:
                    projectiles.append(projectile)

        elif envelope.message == MSG_REQUEST_SENT:
            body, _ = fields(data, envelope)
            if len(body) < 3:
                malformed["RequestSent"] += 1
            else:
                request_count += 1
                if include_event_rows:
                    requests.append(
                        {
                            "eventIndex": event_index,
                            "offset": envelope.offset,
                            "rawEventTime": _rounded(envelope.time),
                            "requestType": body[0].u32,
                            "requestId": body[1].u32,
                            "creatorPlayerId": body[2].u32,
                            "optionalTacticalAidTypeId": (
                                _support_id_json(body[7].u32)
                                if len(body) >= 8 and body[7].u32 != 0
                                else None
                            ),
                            "attribution": {
                                "class": "exact",
                                "playerId": body[2].u32,
                                "basis": "serializedRequestCreator",
                                "scope": "requestOnly",
                            },
                        }
                    )

        previous = envelope

    matches = match_purchase_spawn_candidates(
        purchases, spawns, position_epsilon=position_epsilon
    )
    marker_matches = match_purchase_marker_candidates(
        purchases, markers, position_epsilon=position_epsilon
    )
    match_status = Counter(match["status"] for match in matches)
    marker_match_status = Counter(match["status"] for match in marker_matches)
    marker_stop_links = link_marker_stops(markers, marker_stops)
    marker_stop_status = Counter(
        item["attribution"]["class"] for item in marker_stop_links
    )
    marker_player_matches = 0
    marker_player_mismatches = 0
    for match in marker_matches:
        if match["status"] != "unique" or match["recorderPlayerId"] is None:
            continue
        candidate = match["candidates"][match["selectedCandidateIndex"]]
        if candidate["effectPlayerId"] == match["recorderPlayerId"]:
            marker_player_matches += 1
        else:
            marker_player_mismatches += 1
    span = (
        last_body_end - first_offset
        if last_body_end is not None and first_offset is not None
        else 0
    )

    result = {
        "schemaVersion": 3,
        "path": str(replay_path),
        "sha256": _sha256(replay_path),
        "decompressedBytes": len(data),
        "envelopes": envelope_count,
        "chainCoverage": _rounded(covered_bytes / span) if span else 0.0,
        "recorderPlayerId": recorder,
        "positionEpsilon": position_epsilon,
        "eventRowsIncluded": include_event_rows,
        "purchaseSpawnCandidates": matches,
        "purchaseMarkerCandidates": marker_matches,
        "markerSupportTypes": marker_support_type_stats(markers),
        "metrics": {
            "positionedPurchases": len(purchases),
            "purchasesWithExactRecorderLedger": sum(
                item.honors_delta is not None and item.honors_delta < 0
                for item in purchases
            ),
            "supportSpawnsDelayed": len(spawns),
            "supportMarkers": len(markers),
            "supportMarkersWithValidPlayerSlot": sum(
                0 <= marker.player_id < 16 for marker in markers
            ),
            "supportMarkersWithInvalidPlayerSlot": sum(
                not 0 <= marker.player_id < 16 for marker in markers
            ),
            "supportMarkerStops": len(marker_stops),
            "exactSupportMarkerStopLinks": marker_stop_status["exact"],
            "ambiguousSupportMarkerStopLinks": marker_stop_status["ambiguous"],
            "unmatchedSupportMarkerStops": marker_stop_status["unmatched"],
            "supportFeedback": feedback_count,
            "supportProjectiles": projectile_count,
            "requests": request_count,
            "uniquePurchaseSpawnCandidates": match_status["unique"],
            "ambiguousPurchaseSpawnCandidates": match_status["ambiguous"],
            "unmatchedPurchases": match_status["unmatched"],
            "uniquePurchaseMarkerCandidates": marker_match_status["unique"],
            "ambiguousPurchaseMarkerCandidates": marker_match_status["ambiguous"],
            "unmatchedPurchaseMarkers": marker_match_status["unmatched"],
            "uniquePurchaseMarkerPlayerMatches": marker_player_matches,
            "uniquePurchaseMarkerPlayerMismatches": marker_player_mismatches,
            "malformedMessages": dict(sorted(malformed.items())),
        },
    }
    if include_event_rows:
        result["events"] = {
            "supportCatalogueCount": catalogue_count,
            "recorderPurchases": [_purchase_json(item) for item in purchases],
            "supportSpawnsDelayed": [_spawn_json(item) for item in spawns],
            "supportMarkers": [_marker_json(item) for item in markers],
            "supportMarkerStops": marker_stop_links,
            "supportFeedback": feedback,
            "supportProjectiles": projectiles,
            "requests": requests,
        }
    return result


def analyse_replay_safe(arguments: tuple[str, float, bool]) -> dict:
    """Multiprocessing-safe wrapper that records failures in the result set."""
    path, position_epsilon, include_event_rows = arguments
    try:
        return analyse_replay(
            path,
            position_epsilon=position_epsilon,
            include_event_rows=include_event_rows,
        )
    except Exception as error:  # noqa: BLE001 - private corpus includes damage
        return {
            "schemaVersion": 3,
            "path": path,
            "status": "failed",
            "error": f"{type(error).__name__}: {error}",
        }


def summarise(results: list[dict]) -> dict:
    """Summarise deterministic per-replay evidence rows."""
    totals = Counter()
    support_types: dict[str, Counter] = defaultdict(Counter)
    marker_support_types: dict[str, Counter] = defaultdict(Counter)
    time_deltas = []
    position_distances = []
    deployed_marker_types: dict[str, dict] = {}

    for result in results:
        if result.get("status") == "failed":
            totals["replaysFailed"] += 1
            continue
        totals["replaysAnalysed"] += 1
        for key, value in result["metrics"].items():
            if isinstance(value, int):
                totals[key] += value
        for match in result["purchaseSpawnCandidates"]:
            support = support_types[match["supportTypeId"]]
            support[match["status"]] += 1
            if match["status"] != "unique":
                continue
            candidate = match["candidates"][match["selectedCandidateIndex"]]
            time_deltas.append(candidate["rawTimeDelta"])
            position_distances.append(candidate["positionDistance"])
        for match in result["purchaseMarkerCandidates"]:
            marker_support_types[match["supportTypeId"]][match["status"]] += 1
        for support_type_id, stats in result.get("markerSupportTypes", {}).items():
            aggregate = deployed_marker_types.setdefault(
                support_type_id,
                {
                    "deployments": 0,
                    "replays": 0,
                    "playerSlotCounts": Counter(),
                    "upgradeLevelCounts": Counter(),
                    "minimumDurationSeconds": None,
                    "maximumDurationSeconds": None,
                    "zeroDuration": 0,
                    "zeroDirection": 0,
                    "nonzeroDirection": 0,
                    "worldOriginPosition": 0,
                    "sampleReplays": [],
                },
            )
            aggregate["deployments"] += stats["deployments"]
            aggregate["replays"] += 1
            aggregate["playerSlotCounts"].update(stats["playerSlotCounts"])
            aggregate["upgradeLevelCounts"].update(stats["upgradeLevelCounts"])
            minimum = stats["durationSecondsRange"]["minimum"]
            maximum = stats["durationSecondsRange"]["maximum"]
            if (
                aggregate["minimumDurationSeconds"] is None
                or minimum < aggregate["minimumDurationSeconds"]
            ):
                aggregate["minimumDurationSeconds"] = minimum
            if (
                aggregate["maximumDurationSeconds"] is None
                or maximum > aggregate["maximumDurationSeconds"]
            ):
                aggregate["maximumDurationSeconds"] = maximum
            for key in (
                "zeroDuration",
                "zeroDirection",
                "nonzeroDirection",
                "worldOriginPosition",
            ):
                aggregate[key] += stats[key]
            if len(aggregate["sampleReplays"]) < 5:
                aggregate["sampleReplays"].append(result["path"])

    deployed_marker_report = {}
    for support_type_id, aggregate in sorted(deployed_marker_types.items()):
        deployed_marker_report[support_type_id] = {
            **aggregate,
            "playerSlotCounts": dict(sorted(aggregate["playerSlotCounts"].items())),
            "upgradeLevelCounts": dict(sorted(aggregate["upgradeLevelCounts"].items())),
        }

    return {
        "schemaVersion": 3,
        **dict(sorted(totals.items())),
        "positionEpsilon": results[0].get("positionEpsilon") if results else None,
        "uniqueCandidateTimeDeltaRange": (
            {"minimum": min(time_deltas), "maximum": max(time_deltas)}
            if time_deltas
            else None
        ),
        "uniqueCandidatePositionDistanceRange": (
            {
                "minimum": min(position_distances),
                "maximum": max(position_distances),
            }
            if position_distances
            else None
        ),
        "bySpawnSupportType": {
            support_type: dict(sorted(counts.items()))
            for support_type, counts in sorted(support_types.items())
        },
        "byMarkerSupportType": {
            support_type: dict(sorted(counts.items()))
            for support_type, counts in sorted(marker_support_types.items())
        },
        "byDeployedMarkerSupportType": deployed_marker_report,
        "independentEffectAttributionTruthAvailable": True,
        "incorrectAttributions": totals["uniquePurchaseMarkerPlayerMismatches"],
        "markerPlayerAttributionRuleReady": (
            totals["supportMarkersWithInvalidPlayerSlot"] == 0
            and totals["uniquePurchaseMarkerPlayerMismatches"] == 0
        ),
        "rulePromotionReady": False,
    }
