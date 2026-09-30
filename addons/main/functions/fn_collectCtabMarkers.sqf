/* Normalizes locally visible user markers from supported cTab providers. */
private _edition = missionNamespace getVariable ["CTabWeb_ctabEdition", ""];
if (_edition isEqualTo "") then { _edition = call CTabWeb_fnc_detectCtabEdition; };
if !(_edition in ["original", "devastator", "solar_60th"]) exitWith { [] };
if (isNil "cTabUserMarkerList") exitWith { [] };

private _patchName = ["cTab", "solar_60th_equipment_cTab"] select (_edition isEqualTo "solar_60th");
private _sourceMod = configSourceMod (configFile >> "CfgPatches" >> _patchName);
private _modLookup = missionNamespace getVariable ["CTabWeb_loadedModLookup", createHashMap];
private _modIdentity = _modLookup getOrDefault [toLower _sourceMod, ["0", ""]];
private _exportIconPath = {
    params [["_path", "", [""]]];
    private _pathLower = toLower _path;
    private _isBaseGamePath = (_pathLower find "\a3\") isEqualTo 0
        || { (_pathLower find "a3\") isEqualTo 0 };
    if (
        _path isEqualTo ""
        || { _isBaseGamePath }
        || { _sourceMod isEqualTo "" }
        || { count _sourceMod > 128 }
    ) exitWith { _path };

    private _descriptor = toJSON [
        "ctab_mod_icon_v1",
        _sourceMod,
        _modIdentity param [0, "0", [""]],
        _modIdentity param [1, "", [""]],
        _path
    ];
    [_path, _descriptor] select (count _descriptor <= 512)
};

private _markers = [];
{
    if (count _markers >= 4096) exitWith {};
    if (_x isEqualType [] && { count _x >= 2 }) then {
        private _transactionId = _x select 0;
        private _data = _x select 1;
        if (_transactionId isEqualType 0 && { _transactionId >= 0 } && { _data isEqualType [] } && { count _data >= 7 }) then {
            private _position = _data select 0;
            private _iconPath = _data select 1;
            private _overlayPath = _data select 2;
            private _reportedDirection = _data select 3;
            private _rgba = _data select 4;
            private _label = _data select 5;
            if (_position isEqualType [] && { count _position >= 2 }) then {
                if !(_iconPath isEqualType "") then { _iconPath = ""; };
                if !(_overlayPath isEqualType "") then { _overlayPath = ""; };
                if !(_reportedDirection isEqualType 0) then { _reportedDirection = 400; };
                if !(_label isEqualType "") then { _label = ""; };
                private _color = "#9b1118";
                if (_rgba isEqualType [] && { count _rgba >= 4 }) then {
                    _color = [_rgba, _color] call CTabWeb_fnc_colorToHex;
                };
                private _polyline = [];
                if (_reportedDirection >= 0 && { _reportedDirection < 360 }) then {
                    _polyline pushBack createHashMapFromArray [["x", _position select 0], ["y", _position select 1]];
                    _polyline pushBack createHashMapFromArray [
                        ["x", (_position select 0) + sin _reportedDirection * 250],
                        ["y", (_position select 1) + cos _reportedDirection * 250]
                    ];
                };
                private _direction = [0, _reportedDirection] select (_reportedDirection >= 0 && { _reportedDirection < 360 });
                private _id = format ["ctab-%1-user:%2", _edition, _transactionId];
                private _markerType = [_edition, _iconPath, _overlayPath, _transactionId] call CTabWeb_fnc_ctabMarkerType;
                _iconPath = [_iconPath] call _exportIconPath;
                _overlayPath = [_overlayPath] call _exportIconPath;
                _markers pushBack createHashMapFromArray [
                    ["id", _id],
                    ["label", _label select [0, 512]],
                    ["kind", "icon"],
                    ["position", createHashMapFromArray [["x", _position select 0], ["y", _position select 1]]],
                    ["direction", _direction],
                    ["color", _color],
                    ["alpha", 1],
                    ["marker_type", _markerType],
                    ["icon_path", _iconPath select [0, 512]],
                    ["overlay_icon_path", _overlayPath select [0, 512]],
                    ["brush", "Solid"],
                    ["size", createHashMapFromArray [["x", 1], ["y", 1]]],
                    ["polyline", _polyline],
                    ["channel", -1]
                ];
            };
        };
    };
} forEach (missionNamespace getVariable ["cTabUserMarkerList", []]);

_markers
