/* Publishes one validated protocol envelope without blocking on companion I/O. */
params [["_type", "", [""]], ["_payload", createHashMap, [createHashMap]]];
if (_type isEqualTo "" || isNil { missionNamespace getVariable "CTabWeb_sessionId" }) exitWith { false };

/* Serialize the potentially large payload before entering the short critical section. */
private _serializedPayload = toJSON _payload;
private _serializedType = toJSON _type;
private _serializedSession = toJSON (missionNamespace getVariable "CTabWeb_sessionId");
private _extensionResult = [];
/* Sequence allocation and enqueue must not be interleaved with the other exporters. */
isNil {
    private _sequence = (missionNamespace getVariable ["CTabWeb_sequence", 0]) + 1;
    missionNamespace setVariable ["CTabWeb_sequence", _sequence];
    private _encoded = format [
        "{""protocol_version"":1,""session_id"":%1,""sequence"":%2,""type"":%3,""payload"":%4}",
        _serializedSession, toJSON _sequence, _serializedType, _serializedPayload
    ];
    _extensionResult = "ctab_web_bridge" callExtension ["publish", [_encoded]];
};
_extensionResult params ["_output", "_returnCode", "_errorCode"];
if (_errorCode != 0 || _returnCode != 0) exitWith {
    diag_log format ["[cTab Web Companion] Publish failed (type=%1, return=%2, extension=%3): %4", _type, _returnCode, _errorCode, _output];
    false
};
true
