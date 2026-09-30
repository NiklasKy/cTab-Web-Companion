/* Returns the locally visible Arma marker state in protocol form. */
params [["_marker", "", [""]], ["_knownVisible", false, [true]]];
if (
    _marker isEqualTo ""
    || count _marker > 512
    || { !_knownVisible && { !(_marker in allMapMarkers) } }
) exitWith { nil };

/* A marker may disappear after allMapMarkers was copied. Read its engine fields together. */
private _fields = [];
isNil {
    private _shape = toLower markerShape _marker;
    if (_shape in ["icon", "rectangle", "ellipse", "polyline"]) then {
        _fields = [
            _shape, markerPos _marker, markerSize _marker, markerType _marker,
            markerColor _marker, markerText _marker, markerDir _marker, markerAlpha _marker,
            markerBrush _marker, markerChannel _marker,
            if (_shape isEqualTo "polyline") then { markerPolyline _marker } else { [] }
        ];
    };
};
if (_fields isEqualTo []) exitWith { nil };
_fields params ["_shape", "_position", "_size", "_markerType", "_markerColor", "_text", "_direction", "_alpha", "_brush", "_channel", "_rawPolyline"];
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
private _polyline = [];
for "_index" from 0 to ((count _rawPolyline) - 2) step 2 do {
    _polyline pushBack createHashMapFromArray [
        ["x", _rawPolyline select _index],
        ["y", _rawPolyline select (_index + 1)]
    ];
};

createHashMapFromArray [
    ["id", _marker],
    ["label", _text select [0, 512]],
    ["kind", _shape],
    ["position", createHashMapFromArray [["x", _position select 0], ["y", _position select 1]]],
    ["direction", _direction],
    ["color", _exportColor],
    ["alpha", _alpha],
    ["marker_type", _markerType select [0, 512]],
    ["icon_path", _iconPath select [0, 512]],
    ["overlay_icon_path", ""],
    ["brush", _brush select [0, 512]],
    ["size", createHashMapFromArray [["x", _size select 0], ["y", _size select 1]]],
    ["polyline", _polyline],
    ["channel", _channel]
]
