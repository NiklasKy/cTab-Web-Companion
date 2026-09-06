/* Maps translated cTab texture pairs to stable browser marker identifiers. */
params [
    ["_edition", "unsupported", [""]],
    ["_iconPath", "", [""]],
    ["_overlayPath", "", [""]],
    ["_fallbackId", 0, [0]]
];

private _primary = switch (toLower _iconPath) do {
    case "\a3\ui_f\data\map\markers\nato\o_inf.paa": { "opfor_infantry" };
    case "\a3\ui_f\data\map\markers\nato\o_mech_inf.paa": { "opfor_mechanized_infantry" };
    case "\a3\ui_f\data\map\markers\nato\o_motor_inf.paa": { "opfor_motorized_infantry" };
    case "\a3\ui_f\data\map\markers\nato\o_armor.paa": { "opfor_armor" };
    case "\a3\ui_f\data\map\markers\nato\o_air.paa": { "opfor_air" };
    case "\a3\ui_f\data\map\markers\nato\o_plane.paa": { "opfor_plane" };
    case "\a3\ui_f\data\map\markers\nato\o_unknown.paa": { "opfor_unknown" };
    case "\a3\ui_f\data\map\markers\nato\o_naval.paa": { "opfor_naval" };
    case "\ctab\img\o_inf_rifle.paa": { "opfor_rifle" };
    case "\ctab\img\o_inf_mg.paa": { "opfor_machine_gun" };
    case "\ctab\img\o_inf_at.paa": { "opfor_anti_tank" };
    case "\ctab\img\o_inf_mmg.paa": { "opfor_medium_machine_gun" };
    case "\ctab\img\o_inf_mat.paa": { "opfor_medium_anti_tank" };
    case "\ctab\img\o_inf_mmortar.paa": { "opfor_medium_mortar" };
    case "\ctab\img\o_inf_aa.paa": { "opfor_anti_air" };
    case "\a3\ui_f\data\map\markers\military\join_ca.paa": { "join" };
    case "\a3\ui_f\data\map\markers\military\circle_ca.paa": { "circle" };
    case "\a3\ui_f\data\map\mapcontrol\hospital_ca.paa": { "hospital" };
    case "\a3\ui_f\data\map\markers\military\warning_ca.paa": { "warning" };
    case "\a3\ui_f\data\map\markers\nato\b_hq.paa": { "headquarters" };
    case "\a3\ui_f\data\map\markers\military\end_ca.paa": { "landing_zone" };
    case "\a3\ui_f\data\map\markers\military\pickup_ca.paa": { "logistics_resupply_point" };
    case "\a3\ui_f\data\map\markers\military\marker_ca.paa": { "generic_point" };
    case "\a3\ui_f\data\map\markers\military\objective_ca.paa": { "objective" };
    case "\a3\ui_f\data\map\markers\military\start_ca.paa": { "extraction_point" };
    case "\ctab\img\ckp_ca.paa": { "checkpoint" };
    case "\ctab\img\sp_ca.paa": { "start_point" };
    case "\ctab\img\aa_ca.paa": { "assembly_area" };
    case "\ctab\img\rp_ca.paa": { "release_point" };
    default { format ["unknown_%1", floor _fallbackId] };
};

private _overlay = switch (toLower _overlayPath) do {
    case "\a3\ui_f\data\map\markers\nato\group_0.paa": { "team" };
    case "\a3\ui_f\data\map\markers\nato\group_1.paa": { "squad" };
    case "\a3\ui_f\data\map\markers\nato\group_2.paa": { "section" };
    case "\a3\ui_f\data\map\markers\nato\group_3.paa": { "platoon" };
    default { "none" };
};

format ["ctab_user_%1_%2_%3", _edition, _primary, _overlay]
