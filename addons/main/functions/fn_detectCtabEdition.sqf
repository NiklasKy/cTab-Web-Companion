/* Detects a supported cTab edition through config identity and capabilities. */
private _standardPatch = configFile >> "CfgPatches" >> "cTab";
private _solarPatch = configFile >> "CfgPatches" >> "solar_60th_equipment_cTab";
if (!isClass _standardPatch && { !isClass _solarPatch }) exitWith { "none" };

private _functions = configFile >> "CfgFunctions" >> "cTab" >> "Functions";
private _hasSharedAdapter = isClass (_functions >> "updateLists")
    && { isClass (_functions >> "translateUserMarker") }
    && { isClass (_functions >> "getInfMarkerIcon") };
if (!_hasSharedAdapter) exitWith { "unsupported" };

private _hasPositionCapability = isClass (_functions >> "toggleIfPosition");
if (isClass _solarPatch) exitWith {
    ["unsupported", "solar_60th"] select _hasPositionCapability
};

private _version = getText (_standardPatch >> "versionStr");
private _versionArray = getArray (_standardPatch >> "versionAr");
if (_version isEqualTo "2.2.2.1" && { _versionArray isEqualTo [2, 2, 2, 1] }) exitWith {
    "original"
};

if (_version isEqualTo "2.3.0.0"
    && { _versionArray isEqualTo [2, 3, 0, 0] }
    && { _hasPositionCapability }) exitWith {
    "devastator"
};

"unsupported"
