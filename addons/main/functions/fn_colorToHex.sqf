/* Converts a validated RGBA array to CSS-compatible #RRGGBBAA text. */
params [
    ["_rgba", [], [[]]],
    ["_fallback", "#155A93FF", [""]]
];
if (count _rgba < 4) exitWith { _fallback };
if ((_rgba findIf { !(_x isEqualType 0) }) >= 0) exitWith { _fallback };

private _digits = "0123456789ABCDEF";
private _result = "#";
{
    private _byte = round (((_x max 0) min 1) * 255);
    _result = _result
        + (_digits select [floor (_byte / 16), 1])
        + (_digits select [_byte mod 16, 1]);
} forEach (_rgba select [0, 4]);
_result
