/* Starts the live world, regular-marker, and supported-cTab export. */
if (!hasInterface) exitWith {};

private _pauseMenuHandler = [
    missionNamespace,
    "OnGameInterrupt",
    {
        _this call CTabWeb_fnc_addPauseMenuButton;
    }
] call BIS_fnc_addScriptedEventHandler;
missionNamespace setVariable ["CTabWeb_pauseMenuHandler", _pauseMenuHandler];

[] spawn {
    waitUntil {
        uiSleep 0.1;
        !isNull player || time > 60
    };
    if (isNull player) exitWith {
        diag_log "[cTab Web Companion] No local player became available; export was not started.";
    };

    private _sessionId = format ["arma-%1-%2-%3", clientOwner, floor diag_tickTime, floor random 1000000000];
    missionNamespace setVariable ["CTabWeb_sessionId", _sessionId];
    missionNamespace setVariable ["CTabWeb_sequence", 0];
    missionNamespace setVariable ["CTabWeb_running", true];
    missionNamespace setVariable ["CTabWeb_snapshotInitialized", false];
    missionNamespace setVariable ["CTabWeb_entityState", createHashMap];
    missionNamespace setVariable ["CTabWeb_ctabEntityIds", []];
    missionNamespace setVariable ["CTabWeb_entityTombstones", []];
    missionNamespace setVariable ["CTabWeb_markerState", createHashMap];
    missionNamespace setVariable ["CTabWeb_capabilityState", ""];
    missionNamespace setVariable ["CTabWeb_observedCapabilityState", ""];

    private _loadedModLookup = createHashMap;
    private _decimalDigits = toArray "0123456789";
    {
        private _modDirectory = _x param [1, "", [""]];
        private _modHash = _x param [6, "", [""]];
        private _workshopId = _x param [7, "0", [""]];
        if (
            _workshopId isEqualTo ""
            || count _workshopId > 20
            || { (toArray _workshopId) findIf { !(_x in _decimalDigits) } >= 0 }
        ) then {
            _workshopId = "0";
        };
        if (_modDirectory isNotEqualTo "" && { count _modDirectory <= 128 }) then {
            _loadedModLookup set [toLower _modDirectory, [
                _workshopId select [0, 32],
                _modHash select [0, 64]
            ]];
        };
    } forEach getLoadedModsInfo;
    missionNamespace setVariable ["CTabWeb_loadedModLookup", _loadedModLookup];

    private _ctabEdition = call CTabWeb_fnc_detectCtabEdition;
    missionNamespace setVariable ["CTabWeb_ctabEdition", _ctabEdition];
    switch (_ctabEdition) do {
        case "original": {
            diag_log "[cTab Web Companion] Original cTab 2.2.2.1 adapter detected.";
        };
        case "devastator": {
            diag_log "[cTab Web Companion] cTab Devastator Edition 2.3.0.0 adapter detected.";
        };
        case "unsupported": {
            private _ctabPatch = configFile >> "CfgPatches" >> "cTab";
            private _ctabVersion = getText (_ctabPatch >> "versionStr");
            diag_log format ["[cTab Web Companion] Unsupported cTab edition detected: %1", _ctabVersion];
        };
        default {
            diag_log "[cTab Web Companion] cTab adapter idle because no cTab CfgPatches entry is loaded.";
        };
    };

    private _initialSnapshot = [true] call CTabWeb_fnc_collectSnapshot;
    [_initialSnapshot] call CTabWeb_fnc_publishSnapshot;
    diag_log format [
        "[cTab Web Companion] Initial live snapshot queued (%1 tracked entities, %2 markers).",
        { (_x getOrDefault ["kind", ""]) isNotEqualTo "player" } count (_initialSnapshot get "entities"),
        count (_initialSnapshot get "markers")
    ];
    private _initialMarkers = _initialSnapshot get "markers";
    private _areaMarkers = _initialMarkers select { (_x get "kind") in ["ellipse", "rectangle"] };
    diag_log format [
        "[cTab Web Companion] Marker diagnostics (icons=%1, ellipses=%2, rectangles=%3, polylines=%4, area-zero-alpha=%5, brushes solid=%6, solid-border=%7, border=%8, other=%9).",
        { (_x get "kind") isEqualTo "icon" } count _initialMarkers,
        { (_x get "kind") isEqualTo "ellipse" } count _initialMarkers,
        { (_x get "kind") isEqualTo "rectangle" } count _initialMarkers,
        { (_x get "kind") isEqualTo "polyline" } count _initialMarkers,
        { (_x get "alpha") <= 0 } count _areaMarkers,
        { (toLower (_x get "brush")) isEqualTo "solid" } count _areaMarkers,
        { (toLower (_x get "brush")) isEqualTo "solidborder" } count _areaMarkers,
        { (toLower (_x get "brush")) isEqualTo "border" } count _areaMarkers,
        { !((toLower (_x get "brush")) in ["solid", "solidborder", "border"]) } count _areaMarkers
    ];
    private _reportedModMarkerTypes = createHashMap;
    {
        private _markerType = _x getOrDefault ["marker_type", ""];
        private _markerIcon = _x getOrDefault ["icon_path", ""];
        if (
            (_markerIcon select [0, 1]) isEqualTo "["
            && { !(_markerType in _reportedModMarkerTypes) }
            && { count _reportedModMarkerTypes < 64 }
        ) then {
            _reportedModMarkerTypes set [_markerType, true];
            private _markerConfig = configFile >> "CfgMarkers" >> _markerType;
            diag_log format [
                "[cTab Web Companion] Mod marker resolved (type=%1, source=%2, color=%3).",
                _markerType,
                configSourceMod _markerConfig,
                _x getOrDefault ["color", ""]
            ];
        };
    } forEach _initialMarkers;

    addMissionEventHandler ["Ended", { call CTabWeb_fnc_stopSession; }];
    addMissionEventHandler ["MPEnded", { call CTabWeb_fnc_stopSession; }];

    [] spawn {
        while { missionNamespace getVariable ["CTabWeb_running", false] } do {
            ["heartbeat", createHashMapFromArray [
                ["uptime_ms", diag_tickTime * 1000]
            ]] call CTabWeb_fnc_publish;
            uiSleep 2;
        };
    };

    [] spawn {
        while { missionNamespace getVariable ["CTabWeb_running", false] } do {
            if (!isNull player) then {
                private _capabilities = call CTabWeb_fnc_collectCapabilities;
                private _capabilityState = toJSON [
                    _capabilities getOrDefault ["map", false],
                    _capabilities getOrDefault ["own_position", false],
                    _capabilities getOrDefault ["bft", false]
                ];
                private _observedCapabilityState = missionNamespace getVariable ["CTabWeb_observedCapabilityState", ""];
                if (_capabilityState isNotEqualTo _observedCapabilityState) then {
                    missionNamespace setVariable ["CTabWeb_observedCapabilityState", _capabilityState];
                    private _capabilitySnapshot = [true] call CTabWeb_fnc_collectSnapshot;
                    [_capabilitySnapshot] call CTabWeb_fnc_publishSnapshot;
                } else {
                    private _positionUpdates = [];
                    if (_capabilities getOrDefault ["own_position", false]) then {
                        private _position = getPosWorld player;
                        _positionUpdates pushBack createHashMapFromArray [
                            ["id", "player-local"],
                            ["position", createHashMapFromArray [["x", _position select 0], ["y", _position select 1]]],
                            ["direction", getDirVisual player]
                        ];
                    };
                    if (_capabilities getOrDefault ["bft", false]) then {
                        _positionUpdates append ((call CTabWeb_fnc_collectCtabEntities) apply {
                            createHashMapFromArray [
                                ["id", _x get "id"],
                                ["position", _x get "position"],
                                ["direction", _x get "direction"]
                            ]
                        });
                    };
                    for "_offset" from 0 to ((count _positionUpdates) - 1) step 256 do {
                        ["position_delta", createHashMapFromArray [
                            ["updated", _positionUpdates select [_offset, 256]],
                            ["removed", []]
                        ]] call CTabWeb_fnc_publish;
                    };
                };
            };
            uiSleep 0.25;
        };
    };

    [] spawn {
        uiSleep 2;
        while { missionNamespace getVariable ["CTabWeb_running", false] } do {
            private _snapshot = [true] call CTabWeb_fnc_collectSnapshot;
            [_snapshot] call CTabWeb_fnc_publishSnapshot;
            uiSleep 2;
        };
    };
};
