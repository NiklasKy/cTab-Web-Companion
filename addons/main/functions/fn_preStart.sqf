/*
 * Starts the local companion without blocking Arma on process or pipe work.
 */
private _extensionResult = "ctab_web_bridge" callExtension ["start", []];
_extensionResult params ["_output", "_returnCode", "_errorCode"];

if (_errorCode != 0 || _returnCode != 0) then {
    diag_log format [
        "[cTab Web Companion] Bridge start failed (return=%1, extension=%2): %3",
        _returnCode,
        _errorCode,
        _output
    ];
} else {
    diag_log "[cTab Web Companion] Native bridge accepted the start request.";
};
