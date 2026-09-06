/* Clears exported tactical state when the current mission ends. */
if !(missionNamespace getVariable ["CTabWeb_running", false]) exitWith {};
missionNamespace setVariable ["CTabWeb_running", false];

private _ctabEntityIds = missionNamespace getVariable ["CTabWeb_ctabEntityIds", []];
["entity_delta", createHashMapFromArray [["updated", []], ["removed", _ctabEntityIds]]] call CTabWeb_fnc_publish;
["position_delta", createHashMapFromArray [["updated", []], ["removed", ["player-local"]]]] call CTabWeb_fnc_publish;
private _markerState = missionNamespace getVariable ["CTabWeb_markerState", createHashMap];
private _markerIds = keys _markerState;
for "_offset" from 0 to ((count _markerIds) - 1) step 256 do {
    ["marker_delta", createHashMapFromArray [
        ["updated", []],
        ["removed", _markerIds select [_offset, 256]]
    ]] call CTabWeb_fnc_publish;
};
diag_log "[cTab Web Companion] Mission session stopped and browser state cleared.";
