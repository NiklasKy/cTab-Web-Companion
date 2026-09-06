/* Builds the complete locally authoritative tactical state. */
params [["_refreshCtab", false, [true]]];

private _edition = missionNamespace getVariable ["CTabWeb_ctabEdition", ""];
private _capabilities = call CTabWeb_fnc_collectCapabilities;
private _hasMap = _capabilities getOrDefault ["map", false];
private _hasOwnPosition = _capabilities getOrDefault ["own_position", false];
private _hasBft = _capabilities getOrDefault ["bft", false];
if (_refreshCtab && { _hasBft } && { _edition in ["original", "devastator"] }) then {
    if (!isNil "cTab_fnc_updateLists" && { !isNil "cTab_player" } && { !isNull cTab_player }) then {
        call cTab_fnc_updateLists;
    };
};

private _worldDisplayName = getText (configFile >> "CfgWorlds" >> worldName >> "description");
if (_worldDisplayName isEqualTo "") then { _worldDisplayName = worldName; };
private _currentMissionName = missionName;
if (_currentMissionName isEqualTo "") then { _currentMissionName = "Arma 3 Mission"; };
private _sideName = switch (side group player) do {
    case west: { "west" };
    case east: { "east" };
    case independent: { "independent" };
    case civilian: { "civilian" };
    default { "unknown" };
};
private _playerPosition = getPosWorld player;
private _playerLabel = name player;
if (_playerLabel isEqualTo "") then { _playerLabel = "You"; };
private _playerRecord = createHashMapFromArray [
    ["id", "player-local"],
    ["label", _playerLabel select [0, 512]],
    ["kind", "player"],
    ["position", createHashMapFromArray [["x", _playerPosition select 0], ["y", _playerPosition select 1]]],
    ["direction", getDirVisual player],
    ["side", _sideName],
    ["color", switch (_sideName) do {
        case "west": { "#155a93" };
        case "east": { "#9b1118" };
        case "independent": { "#16812b" };
        case "civilian": { "#75139a" };
        default { "#b59a00" };
    }],
    ["icon_path", ""],
    ["overlay_icon_path", ""]
];

private _entities = [];
if (_hasOwnPosition) then { _entities pushBack _playerRecord; };
if (_hasBft) then {
    {
        if (count _entities >= 2048) exitWith {};
        _entities pushBack _x;
    } forEach (call CTabWeb_fnc_collectCtabEntities);
};

private _markers = [];
private _markerIds = createHashMap;
if (_hasMap) then {
    private _visibleMapMarkers = allMapMarkers;
    {
        if (count _markers >= 4096) exitWith {};
        private _record = [_x, true] call CTabWeb_fnc_collectMarker;
        if (!isNil "_record") then {
            private _id = _record get "id";
            if !(_markerIds getOrDefault [_id, false]) then {
                _markerIds set [_id, true];
                _markers pushBack _record;
            };
        };
    } forEach _visibleMapMarkers;
};
if (_hasBft) then {
    {
        if (count _markers >= 4096) exitWith {};
        private _id = _x get "id";
        if !(_markerIds getOrDefault [_id, false]) then {
            _markerIds set [_id, true];
            _markers pushBack _x;
        };
    } forEach (call CTabWeb_fnc_collectCtabMarkers);
};

createHashMapFromArray [
    ["mission_name", _currentMissionName select [0, 512]],
    ["ctab_edition", _edition],
    ["capabilities", _capabilities],
    ["terrain", createHashMapFromArray [
        ["world_name", worldName],
        ["display_name", _worldDisplayName select [0, 512]],
        ["world_size", worldSize]
    ]],
    ["entities", _entities],
    ["markers", _markers]
]
