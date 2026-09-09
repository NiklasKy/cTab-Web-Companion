class CfgPatches
{
    class ctab_web_main
    {
        name = "cTab Web Companion";
        author = "[GRP9] NiklasKy";
        requiredVersion = 2.18;
        requiredAddons[] = {};
        units[] = {};
        weapons[] = {};
        version = "1.0.1";
        versionStr = "1.0.1";
        versionAr[] = { 1, 0, 1 };
    };
};

class CfgFunctions
{
    class CTabWeb
    {
        class Main
        {
            file = "\z\ctab_web\addons\main\functions";

            class preStart
            {
                preStart = 1;
            };

            class postInit
            {
                postInit = 1;
            };

            class publish {};
            class openBrowser {};
            class addPauseMenuButton {};
            class collectMarker {};
            class entityId {};
            class colorToHex {};
            class detectCtabEdition {};
            class collectCapabilities {};
            class ctabMarkerType {};
            class collectCtabEntities {};
            class collectCtabMarkers {};
            class collectSnapshot {};
            class publishSnapshot {};
            class stopSession {};
        };
    };
};
