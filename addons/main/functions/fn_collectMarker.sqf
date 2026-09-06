/* Returns the locally visible Arma marker state in protocol form. */
params [["_marker", "", [""]], ["_knownVisible", false, [true]]];
if (
    _marker isEqualTo ""
    || count _marker > 512
    || { !_knownVisible && { !(_marker in allMapMarkers) } }
) exitWith { nil };

private _shape = toLower markerShape _marker;
if !(_shape in ["icon", "rectangle", "ellipse", "polyline"]) then { _shape = "icon"; };
private _position = markerPos _marker;
private _size = markerSize _marker;
private _markerType = markerType _marker;
private _markerConfig = configFile >> "CfgMarkers" >> _markerType;
private _iconPath = if (_markerType isEqualTo "") then { "" } else {
    getText (_markerConfig >> "icon")
};
private _pathLower = toLower _iconPath;
private _isBaseGamePath = (_pathLower find "\a3\") isEqualTo 0
    || { (_pathLower find "a3\") isEqualTo 0 };
if (_iconPath isNotEqualTo "" && { !_isBaseGamePath }) then {
    private _sourceMod = configSourceMod _markerConfig;
    if (_sourceMod isNotEqualTo "" && { count _sourceMod <= 128 }) then {
        private _modLookup = missionNamespace getVariable ["CTabWeb_loadedModLookup", createHashMap];
        private _modIdentity = _modLookup getOrDefault [toLower _sourceMod, ["0", ""]];
        private _descriptor = toJSON [
            "ctab_mod_icon_v1",
            _sourceMod,
            _modIdentity param [0, "0", [""]],
            _modIdentity param [1, "", [""]],
            _iconPath
        ];
        if (count _descriptor <= 512) then {
            _iconPath = _descriptor;
        };
    };
};
private _markerColor = markerColor _marker;
private _exportColor = _markerColor;
if ((_markerColor select [0, 1]) isNotEqualTo "#") then {
    private _colorConfig = if ((toLower _markerColor) in ["", "default"]) then {
        _markerConfig >> "color"
    } else {
        configFile >> "CfgMarkerColors" >> _markerColor >> "color"
    };
    if (!isArray _colorConfig) then {
        _colorConfig = _markerConfig >> "color";
    };
    if (isArray _colorConfig) then {
        private _rgba = _colorConfig call BIS_fnc_colorConfigToRGBA;
        _exportColor = [_rgba, _markerColor] call CTabWeb_fnc_colorToHex;
    };
};
private _rawPolyline = if (_shape isEqualTo "polyline") then { markerPolyline _marker } else { [] };
private _polyline = [];
for "_index" from 0 to ((count _rawPolyline) - 2) step 2 do {
    _polyline pushBack createHashMapFromArray [
        ["x", _rawPolyline select _index],
        ["y", _rawPolyline select (_index + 1)]
    ];
};

createHashMapFromArray [
    ["id", _marker],
    ["label", (markerText _marker) select [0, 512]],
    ["kind", _shape],
    ["position", createHashMapFromArray [["x", _position select 0], ["y", _position select 1]]],
    ["direction", markerDir _marker],
    ["color", _exportColor],
    ["alpha", markerAlpha _marker],
    ["marker_type", _markerType select [0, 512]],
    ["icon_path", _iconPath select [0, 512]],
    ["overlay_icon_path", ""],
    ["brush", (markerBrush _marker) select [0, 512]],
    ["size", createHashMapFromArray [["x", _size select 0], ["y", _size select 1]]],
    ["polyline", _polyline],
    ["channel", markerChannel _marker]
]
