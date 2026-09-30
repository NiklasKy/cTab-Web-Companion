/* Adds the cWEB reopen action to the vanilla mission pause menu. */
disableSerialization;
params [["_display", displayNull, [displayNull]]];

if (isNull _display || { !(missionNamespace getVariable ["CTabWeb_running", false]) }) exitWith {
    controlNull
};

private _idc = 891000;
private _button = _display displayCtrl _idc;
if (!isNull _button) exitWith { _button };
if (isNull (_display displayCtrl 2)) exitWith { controlNull };

_button = _display ctrlCreate ["RscButtonMenu", _idc];
_button ctrlSetText "OPEN cWEB";
_button ctrlSetTooltip "Reopen cTab Web Companion in the default browser";

/* Make room above the existing menu: cWEB takes Continue's former row. */
private _layout = {
    params ["_display"];
    private _continue = _display displayCtrl 2;
    private _button = _display displayCtrl 891000;
    if (isNull _continue || { isNull _button }) exitWith {};
    private _position = ctrlPosition _continue;
    private _rowStep = (_position select 3) * 1.1;
    private _originalPositions = [];
    {
        private _control = _display displayCtrl _x;
        if (!isNull _control) then {
            private _original = ctrlPosition _control;
            _originalPositions pushBack [_x, +_original];
            _original set [1, (_original select 1) - _rowStep];
            _control ctrlSetPosition _original;
            _control ctrlCommit 0;
        };
    } forEach [1050, 523, 109, 2]; // Title background, title, player name, Continue.
    _display setVariable ["CTabWeb_pauseMenuOriginalPositions", _originalPositions];
    _button ctrlSetPosition _position;
    _button ctrlCommit 0;
    _button ctrlShow true;
};
_display setVariable ["CTabWeb_layoutPauseMenu", _layout];
[_display] call _layout;

/* Vanilla Configure animates these rows to fixed positions. Reinsert after it settles. */
(_display displayCtrl 101) ctrlAddEventHandler ["ButtonClick", {
    private _display = ctrlParent (_this select 0);
    if (_display getVariable ["CTabWeb_pauseMenuLayoutPending", false]) exitWith {};
    _display setVariable ["CTabWeb_pauseMenuLayoutPending", true];
    (_display displayCtrl 891000) ctrlShow false;
    {
        _x params ["_idc", "_position"];
        private _control = _display displayCtrl _idc;
        _control ctrlSetPosition _position;
        _control ctrlCommit 0;
    } forEach (_display getVariable ["CTabWeb_pauseMenuOriginalPositions", []]);
    [_display] spawn {
        disableSerialization;
        params ["_display"];
        uiSleep 0.5;
        waitUntil {
            uiSleep 0.05;
            isNull _display || { ctrlCommitted (_display displayCtrl 2) }
        };
        if (!isNull _display) then {
            [_display] call (_display getVariable ["CTabWeb_layoutPauseMenu", {}]);
            _display setVariable ["CTabWeb_pauseMenuLayoutPending", false];
        };
    };
}];
_button ctrlAddEventHandler ["ButtonClick", {
    call CTabWeb_fnc_openBrowser;
}];

_button
