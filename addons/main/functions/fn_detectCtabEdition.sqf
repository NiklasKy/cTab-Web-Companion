/* Detects a supported cTab edition through versioned config capabilities. */
private _patch = configFile >> "CfgPatches" >> "cTab";
if (!isClass _patch) exitWith { "none" };

private _functions = configFile >> "CfgFunctions" >> "cTab" >> "Functions";
private _hasSharedAdapter = isClass (_functions >> "updateLists")
    && { isClass (_functions >> "translateUserMarker") }
    && { isClass (_functions >> "getInfMarkerIcon") };
if (!_hasSharedAdapter) exitWith { "unsupported" };

private _version = getText (_patch >> "versionStr");
private _versionArray = getArray (_patch >> "versionAr");
if (_version isEqualTo "2.2.2.1" && { _versionArray isEqualTo [2, 2, 2, 1] }) exitWith {
    "original"
};

private _hasDevastatorCapability = isClass (_functions >> "toggleIfPosition");
if (_version isEqualTo "2.3.0.0"
    && { _versionArray isEqualTo [2, 3, 0, 0] }
    && { _hasDevastatorCapability }) exitWith {
    "devastator"
};

"unsupported"
