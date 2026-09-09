/* Adds the cWEB reopen action to the vanilla mission pause menu. */
disableSerialization;
params [["_display", displayNull, [displayNull]]];

if (isNull _display || { !(missionNamespace getVariable ["CTabWeb_running", false]) }) exitWith {
    controlNull
};

private _idc = 891000;
private _button = _display displayCtrl _idc;
if (!isNull _button) exitWith { _button };

_button = _display ctrlCreate ["RscButtonMenu", _idc];
_button ctrlSetText "OPEN cWEB";
_button ctrlSetTooltip "Reopen cTab Web Companion in the default browser";

private _aspectWidth = (safeZoneW / safeZoneH) min 1.2;
private _gridWidth = _aspectWidth / 40;
private _gridHeight = (_aspectWidth / 1.2) / 25;
private _gridTop = safeZoneY + safeZoneH - (_aspectWidth / 1.2);
_button ctrlSetPosition [
    safeZoneX + _gridWidth,
    _gridTop + (21.9 * _gridHeight),
    15 * _gridWidth,
    _gridHeight
];
_button ctrlCommit 0;
_button ctrlAddEventHandler ["ButtonClick", {
    call CTabWeb_fnc_openBrowser;
}];

_button
