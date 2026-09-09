/* Requests that the running companion reopen its protected browser URL. */
private _extensionResult = "ctab_web_bridge" callExtension ["open_browser", []];
_extensionResult params ["_output", "_returnCode", "_errorCode"];

if (_errorCode != 0 || _returnCode != 0) exitWith {
    diag_log format [
        "[cTab Web Companion] Browser reopen failed (return=%1, extension=%2): %3",
        _returnCode,
        _errorCode,
        _output
    ];
    systemChat "cWEB: The browser could not be opened. Check the Arma RPT for details.";
    false
};

diag_log "[cTab Web Companion] Browser reopen requested from the pause menu.";
true
