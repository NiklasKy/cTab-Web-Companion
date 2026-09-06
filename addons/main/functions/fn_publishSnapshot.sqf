/* Publishes the initial authority once, then only changed tactical records. */
params [["_snapshot", createHashMap, [createHashMap]]];

private _entities = _snapshot getOrDefault ["entities", []];
private _markers = _snapshot getOrDefault ["markers", []];
private _capabilities = _snapshot getOrDefault ["capabilities", createHashMap];
private _capabilityState = toJSON [
    _capabilities getOrDefault ["map", false],
    _capabilities getOrDefault ["own_position", false],
    _capabilities getOrDefault ["bft", false]
];
private _previousCapabilityState = missionNamespace getVariable ["CTabWeb_capabilityState", ""];
if (_previousCapabilityState isNotEqualTo "" && { _capabilityState isNotEqualTo _previousCapabilityState }) then {
    missionNamespace setVariable ["CTabWeb_snapshotInitialized", false];
};
private _entityState = createHashMap;
private _markerState = createHashMap;
private _entityIds = [];

{
    private _id = _x get "id";
    private _visualSignature = toJSON [
        _x getOrDefault ["label", ""],
        _x getOrDefault ["kind", ""],
        _x getOrDefault ["side", ""],
        _x getOrDefault ["color", ""],
        _x getOrDefault ["icon_path", ""],
        _x getOrDefault ["overlay_icon_path", ""]
    ];
    _entityIds pushBack _id;
    _entityState set [_id, _visualSignature];
} forEach _entities;

{
    private _id = _x get "id";
    private _position = _x getOrDefault ["position", createHashMap];
    private _size = _x getOrDefault ["size", createHashMap];
    private _polylineSignature = (_x getOrDefault ["polyline", []]) apply {
        [_x getOrDefault ["x", 0], _x getOrDefault ["y", 0]]
    };
    private _signature = toJSON [
        _x getOrDefault ["label", ""],
        _x getOrDefault ["kind", ""],
        _position getOrDefault ["x", 0],
        _position getOrDefault ["y", 0],
        _x getOrDefault ["direction", 0],
        _x getOrDefault ["color", ""],
        _x getOrDefault ["alpha", 1],
        _x getOrDefault ["marker_type", ""],
        _x getOrDefault ["icon_path", ""],
        _x getOrDefault ["overlay_icon_path", ""],
        _x getOrDefault ["brush", ""],
        _size getOrDefault ["x", 0],
        _size getOrDefault ["y", 0],
        _polylineSignature,
        _x getOrDefault ["channel", -1]
    ];
    _markerState set [_id, _signature];
} forEach _markers;

if !(missionNamespace getVariable ["CTabWeb_snapshotInitialized", false]) exitWith {
    private _serialized = toJSON _snapshot;
    private _success = false;

    /* Leave ample space below the native 256 KiB frame limit for UTF-8 and framing. */
    if (count _serialized <= 60000) then {
        _success = ["session_snapshot", _snapshot] call CTabWeb_fnc_publish;
    } else {
        /* BFT state belongs in the first frame so a joining browser never starts without it. */
        private _baseSnapshot = createHashMapFromArray [
            ["mission_name", _snapshot getOrDefault ["mission_name", "Arma 3 Mission"]],
            ["ctab_edition", _snapshot getOrDefault ["ctab_edition", "none"]],
            ["capabilities", _capabilities],
            ["terrain", _snapshot getOrDefault ["terrain", createHashMap]],
            ["entities", _entities],
            ["markers", []]
        ];
        private _baseSerialized = toJSON _baseSnapshot;
        private _deferredEntities = [];
        if (count _baseSerialized > 60000) then {
            private _playerEntities = _entities select {
                (_x getOrDefault ["kind", ""]) isEqualTo "player"
            };
            _deferredEntities = _entities select {
                (_x getOrDefault ["kind", ""]) isNotEqualTo "player"
            };
            _baseSnapshot set ["entities", _playerEntities];
        };

        _success = ["session_snapshot", _baseSnapshot] call CTabWeb_fnc_publish;
        for "_offset" from 0 to ((count _deferredEntities) - 1) step 128 do {
            _success = (["entity_delta", createHashMapFromArray [
                ["updated", _deferredEntities select [_offset, 128]],
                ["removed", []]
            ]] call CTabWeb_fnc_publish) && _success;
        };
        for "_offset" from 0 to ((count _markers) - 1) step 128 do {
            _success = (["marker_delta", createHashMapFromArray [
                ["updated", _markers select [_offset, 128]],
                ["removed", []]
            ]] call CTabWeb_fnc_publish) && _success;
        };
    };

    if (_success) then {
        missionNamespace setVariable ["CTabWeb_snapshotInitialized", true];
        missionNamespace setVariable ["CTabWeb_entityState", _entityState];
        missionNamespace setVariable ["CTabWeb_ctabEntityIds", _entityIds];
        missionNamespace setVariable ["CTabWeb_entityTombstones", []];
        missionNamespace setVariable ["CTabWeb_markerState", _markerState];
        missionNamespace setVariable ["CTabWeb_capabilityState", _capabilityState];
    };
    _success
};

private _previousEntityState = missionNamespace getVariable ["CTabWeb_entityState", createHashMap];
private _previousEntityIds = missionNamespace getVariable ["CTabWeb_ctabEntityIds", []];
private _previousMarkerState = missionNamespace getVariable ["CTabWeb_markerState", createHashMap];
private _updatedEntities = _entities select {
    private _id = _x get "id";
    (_previousEntityState getOrDefault [_id, ""]) isNotEqualTo (_entityState get _id)
};
private _removedEntities = _previousEntityIds select {
    (_entityState getOrDefault [_x, ""]) isEqualTo ""
};
/*
 * Repeat entity removals across several full collection cycles. The native
 * bridge is asynchronous, and this bounded tombstone window makes removals
 * self-healing without periodically replacing the complete marker snapshot.
 */
private _entityTombstones = missionNamespace getVariable ["CTabWeb_entityTombstones", []];
private _activeTombstones = [];
{
    if (_x isEqualType [] && { count _x >= 2 }) then {
        private _retiredId = _x select 0;
        private _retries = _x select 1;
        if (
            _retiredId isEqualType ""
            && { _retiredId isNotEqualTo "" }
            && { _retries isEqualType 0 }
            && { _retries > 0 }
            && { !(_retiredId in _entityIds) }
        ) then {
            _activeTombstones pushBack [_retiredId, _retries];
        };
    };
} forEach _entityTombstones;
{
    private _retiredId = _x;
    private _existingIndex = _activeTombstones findIf { (_x select 0) isEqualTo _retiredId };
    if (_existingIndex < 0) then {
        _activeTombstones pushBack [_retiredId, 5];
    } else {
        _activeTombstones set [_existingIndex, [_retiredId, 5]];
    };
} forEach _removedEntities;
if (count _activeTombstones > 2048) then {
    _activeTombstones = _activeTombstones select [(count _activeTombstones) - 2048, 2048];
};
private _removalIds = _activeTombstones apply { _x select 0 };
private _updatedMarkers = _markers select {
    private _id = _x get "id";
    (_previousMarkerState getOrDefault [_id, ""]) isNotEqualTo (_markerState get _id)
};
private _removedMarkers = (keys _previousMarkerState) select {
    (_markerState getOrDefault [_x, ""]) isEqualTo ""
};
private _success = true;

for "_offset" from 0 to ((count _updatedEntities) - 1) step 128 do {
    _success = (["entity_delta", createHashMapFromArray [
        ["updated", _updatedEntities select [_offset, 128]],
        ["removed", []]
    ]] call CTabWeb_fnc_publish) && _success;
};
for "_offset" from 0 to ((count _removalIds) - 1) step 128 do {
    _success = (["entity_delta", createHashMapFromArray [
        ["updated", []],
        ["removed", _removalIds select [_offset, 128]]
    ]] call CTabWeb_fnc_publish) && _success;
};
for "_offset" from 0 to ((count _updatedMarkers) - 1) step 128 do {
    _success = (["marker_delta", createHashMapFromArray [
        ["updated", _updatedMarkers select [_offset, 128]],
        ["removed", []]
    ]] call CTabWeb_fnc_publish) && _success;
};
for "_offset" from 0 to ((count _removedMarkers) - 1) step 128 do {
    _success = (["marker_delta", createHashMapFromArray [
        ["updated", []],
        ["removed", _removedMarkers select [_offset, 128]]
    ]] call CTabWeb_fnc_publish) && _success;
};

if (_success) then {
    private _remainingTombstones = [];
    {
        private _retries = (_x select 1) - 1;
        if (_retries > 0) then {
            _remainingTombstones pushBack [_x select 0, _retries];
        };
    } forEach _activeTombstones;
    missionNamespace setVariable ["CTabWeb_entityState", _entityState];
    missionNamespace setVariable ["CTabWeb_ctabEntityIds", _entityIds];
    missionNamespace setVariable ["CTabWeb_entityTombstones", _remainingTombstones];
    missionNamespace setVariable ["CTabWeb_markerState", _markerState];
    missionNamespace setVariable ["CTabWeb_capabilityState", _capabilityState];
};
_success
