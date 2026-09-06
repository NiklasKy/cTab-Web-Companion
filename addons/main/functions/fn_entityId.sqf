/* Returns a session-stable identifier for an engine object. */
params [
    ["_object", objNull, [objNull]],
    ["_prefix", "ctab", [""]]
];
if (isNull _object || _prefix isEqualTo "") exitWith { "" };

private _networkId = netId _object;
if (_networkId isEqualTo "" || _networkId isEqualTo "0:0") then {
    _networkId = str (hashValue _object);
};
format ["%1:%2", _prefix, _networkId]
