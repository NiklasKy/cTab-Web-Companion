/* Publishes one validated protocol envelope without blocking on companion I/O. */
params [["_type", "", [""]], ["_payload", createHashMap, [createHashMap]]];
if (_type isEqualTo "" || isNil { missionNamespace getVariable "CTabWeb_sessionId" }) exitWith { false };

private _sequence = (missionNamespace getVariable ["CTabWeb_sequence", 0]) + 1;
missionNamespace setVariable ["CTabWeb_sequence", _sequence];
private _envelope = createHashMapFromArray [
    ["protocol_version", 1],
    ["session_id", missionNamespace getVariable "CTabWeb_sessionId"],
    ["sequence", _sequence],
    ["type", _type],
    ["payload", _payload]
];
private _extensionResult = "ctab_web_bridge" callExtension ["publish", [toJSON _envelope]];
_extensionResult params ["_output", "_returnCode", "_errorCode"];
if (_errorCode != 0 || _returnCode != 0) exitWith {
    diag_log format ["[cTab Web Companion] Publish failed (type=%1, return=%2, extension=%3): %4", _type, _returnCode, _errorCode, _output];
    false
};
true
