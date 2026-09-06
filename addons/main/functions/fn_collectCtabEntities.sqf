/* Normalizes locally available BFT records from either supported cTab edition. */
private _edition = missionNamespace getVariable ["CTabWeb_ctabEdition", ""];
if (_edition isEqualTo "") then { _edition = call CTabWeb_fnc_detectCtabEdition; };
if !(_edition in ["original", "devastator"]) exitWith { [] };
if (isNil "cTabBFTmembers" || { isNil "cTabBFTgroups" } || { isNil "cTabBFTvehicles" }) exitWith { [] };
private _idNamespace = format ["ctab-%1", _edition];

private _sideName = {
    params ["_side"];
    switch (_side) do {
        case west: { "west" };
        case east: { "east" };
        case independent: { "independent" };
        case civilian: { "civilian" };
        default { "unknown" };
    }
};
private _sideColor = {
    params ["_side"];
    switch (_side) do {
        case west: { "#155a93" };
        case east: { "#9b1118" };
        case independent: { "#16812b" };
        case civilian: { "#75139a" };
        default { "#b59a00" };
    }
};
private _blueRgba = missionNamespace getVariable ["cTabColorBlue", [0, 0.8, 1, 0.8]];
private _blueColor = [_blueRgba, "#00CCFFCC"] call CTabWeb_fnc_colorToHex;
private _teamColors = missionNamespace getVariable ["cTabColorTeam", []];
private _entities = [];
private _trackedVehicles = [];
private _ctabPlayer = if (!isNil "cTab_player" && { !isNull cTab_player }) then { cTab_player } else { player };
private _playerGroup = group _ctabPlayer;
private _playerVehicle = vehicle _ctabPlayer;
private _liveGroups = allGroups;

{
    if (count _entities >= 2047) exitWith {};
    if (_x isEqualType [] && { count _x >= 5 }) then {
        private _object = _x select 0;
        if (_object isEqualType objNull && { !isNull _object }) then {
            private _iconPath = _x select 1;
            private _overlayPath = _x select 2;
            private _label = _x select 3;
            if !(_iconPath isEqualType "") then { _iconPath = ""; };
            if !(_overlayPath isEqualType "") then { _overlayPath = ""; };
            if !(_label isEqualType "") then { _label = "Vehicle"; };
            if (_label isEqualTo "") then { _label = getText (configOf _object >> "displayName"); };
            if (_label isEqualTo "") then { _label = "Vehicle"; };
            private _position = getPosWorld _object;
            private _objectSide = side _object;
            _entities pushBack createHashMapFromArray [
                ["id", [_object, _idNamespace + "-vehicle"] call CTabWeb_fnc_entityId],
                ["label", _label select [0, 512]],
                ["kind", "bft_vehicle"],
                ["position", createHashMapFromArray [["x", _position select 0], ["y", _position select 1]]],
                ["direction", 0],
                ["side", [_objectSide] call _sideName],
                ["color", _blueColor],
                ["icon_path", _iconPath select [0, 512]],
                ["overlay_icon_path", _overlayPath select [0, 512]]
            ];
            _trackedVehicles pushBack _object;
        };
    };
} forEach (missionNamespace getVariable ["cTabBFTvehicles", []]);

{
    if (count _entities >= 2047) exitWith {};
    if (_x isEqualType [] && { count _x >= 5 }) then {
        private _leader = _x select 0;
        if (_leader isEqualType objNull && { !isNull _leader }) then {
            private _iconPath = _x select 1;
            private _overlayPath = _x select 2;
            private _label = _x select 3;
            if !(_iconPath isEqualType "") then { _iconPath = ""; };
            if !(_overlayPath isEqualType "") then { _overlayPath = ""; };
            if !(_label isEqualType "") then { _label = "Group"; };
            if (_label isEqualTo "") then { _label = "Group"; };

            /*
             * cTab caches only the former positioning unit and group label. A
             * unit can leave that group while the cached row still exists, so
             * validate both values against Arma's current group inventory.
             */
            private _liveGroup = group _leader;
            private _isCurrentGroupRecord = _liveGroup in _liveGroups
                && { units _liveGroup isNotEqualTo [] }
                && { _leader in units _liveGroup }
                && { _label isEqualTo (groupId _liveGroup) };
            private _leaderVehicle = vehicle _leader;
            if (
                _isCurrentGroupRecord
                && { _liveGroup isNotEqualTo _playerGroup }
                && { !(_leaderVehicle in _trackedVehicles) }
                && { _leaderVehicle isNotEqualTo _playerVehicle }
            ) then {
                private _position = getPosWorld _leaderVehicle;
                private _objectSide = side _liveGroup;
                /*
                 * cTab may select a different tablet carrier for the same group.
                 * Anchor the web identity to the group itself so that a carrier
                 * change moves one marker instead of creating a trail of markers.
                 */
                private _groupNetworkId = netId _liveGroup;
                if (_groupNetworkId isEqualTo "" || { _groupNetworkId isEqualTo "0:0" }) then {
                    _groupNetworkId = str (hashValue _liveGroup);
                };
                _entities pushBack createHashMapFromArray [
                    ["id", format ["%1-group:%2", _idNamespace, _groupNetworkId]],
                    ["label", _label select [0, 512]],
                    ["kind", "bft_unit"],
                    ["position", createHashMapFromArray [["x", _position select 0], ["y", _position select 1]]],
                    ["direction", 0],
                    ["side", [_objectSide] call _sideName],
                    ["color", _blueColor],
                    ["icon_path", _iconPath select [0, 512]],
                    ["overlay_icon_path", _overlayPath select [0, 512]]
                ];
            };
        };
    };
} forEach (missionNamespace getVariable ["cTabBFTgroups", []]);

{
    if (count _entities >= 2047) exitWith {};
    if (_x isEqualType [] && { count _x >= 5 }) then {
        private _unit = _x select 0;
        if (_unit isEqualType objNull && { !isNull _unit }) then {
            private _unitVehicle = vehicle _unit;
            if (
                group _unit isEqualTo _playerGroup
                && { !(_unitVehicle in _trackedVehicles) }
                && { _unitVehicle isNotEqualTo _playerVehicle }
            ) then {
                private _iconPath = _x select 1;
                private _label = _x select 3;
                if !(_iconPath isEqualType "") then { _iconPath = ""; };
                if !(_label isEqualType "") then { _label = "Unit"; };
                if (_label isEqualTo "") then { _label = "Unit"; };
                private _position = getPosWorld _unitVehicle;
                private _objectSide = side group _unit;
                private _teamIndex = ["MAIN", "RED", "GREEN", "BLUE", "YELLOW"] find assignedTeam _unit;
                if (_teamIndex < 0) then { _teamIndex = 0; };
                private _teamColor = [_objectSide] call _sideColor;
                if (_teamIndex < count _teamColors) then {
                    private _teamRgba = _teamColors select _teamIndex;
                    if (_teamRgba isEqualType [] && { count _teamRgba >= 4 }) then {
                        _teamColor = [_teamRgba, _teamColor] call CTabWeb_fnc_colorToHex;
                    };
                };
                _entities pushBack createHashMapFromArray [
                    ["id", [_unit, _idNamespace + "-unit"] call CTabWeb_fnc_entityId],
                    ["label", _label select [0, 512]],
                    ["kind", "bft_unit"],
                    ["position", createHashMapFromArray [["x", _position select 0], ["y", _position select 1]]],
                    ["direction", getDirVisual _unitVehicle],
                    ["side", [_objectSide] call _sideName],
                    ["color", _teamColor],
                    ["icon_path", _iconPath select [0, 512]],
                    ["overlay_icon_path", ""]
                ];
            };
        };
    };
} forEach (missionNamespace getVariable ["cTabBFTmembers", []]);

private _deduplicated = [];
private _entityIndices = createHashMap;
{
    private _id = _x getOrDefault ["id", ""];
    if (_id isNotEqualTo "") then {
        private _index = _entityIndices getOrDefault [_id, -1];
        if (_index < 0) then {
            _entityIndices set [_id, count _deduplicated];
            _deduplicated pushBack _x;
        } else {
            _deduplicated set [_index, _x];
        };
    };
} forEach _entities;

_deduplicated
