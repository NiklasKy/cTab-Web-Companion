/* Resolves which tactical data the local player's current equipment may expose. */
private _unit = missionNamespace getVariable ["cTab_player", player];
if (isNull _unit) then { _unit = player; };

private _allItems = items _unit;
_allItems append (assignedItems _unit);
_allItems pushBack (goggles _unit);

private _hasMapItem = false;
private _hasGpsItem = false;
private _hasCtabDevice = false;
private _ctabDeviceClasses = ["itemctab", "itemandroid", "itemmicrodagr"];

{
    private _className = toLower _x;
    if (_className in _ctabDeviceClasses) then {
        _hasCtabDevice = true;
    };

    private _simulation = toLower getText (configFile >> "CfgWeapons" >> _x >> "simulation");
    if (_simulation isEqualTo "itemmap") then { _hasMapItem = true; };
    if (_simulation isEqualTo "itemgps") then { _hasGpsItem = true; };
} forEach _allItems;

private _hasVehicleBft = false;
if (!isNil "cTab_fnc_unitInEnabledVehicleSeat") then {
    private _vehicle = vehicle _unit;
    if (_vehicle isNotEqualTo _unit) then {
        _hasVehicleBft = ([_unit, _vehicle, "FBCB2"] call cTab_fnc_unitInEnabledVehicleSeat)
            || ([_unit, _vehicle, "TAD"] call cTab_fnc_unitInEnabledVehicleSeat);
    };
};

private _hasBft = _hasCtabDevice || _hasVehicleBft;
createHashMapFromArray [
    ["map", _hasMapItem || _hasBft],
    ["own_position", _hasGpsItem || _hasBft],
    ["bft", _hasBft]
]
