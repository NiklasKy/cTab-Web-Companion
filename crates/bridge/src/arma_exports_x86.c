#if !defined(_M_IX86)
#error This ABI shim must only be built for 32-bit x86 Windows.
#endif

extern void __cdecl ctab_web_rv_extension_version_impl(char *output, int output_size);
extern void __cdecl ctab_web_rv_extension_impl(
    char *output,
    int output_size,
    const char *function
);
extern int __cdecl ctab_web_rv_extension_args_impl(
    char *output,
    int output_size,
    const char *function,
    const char **arguments,
    int argument_count
);

__declspec(dllexport) void __stdcall RVExtensionVersion(char *output, int output_size)
{
    ctab_web_rv_extension_version_impl(output, output_size);
}

__declspec(dllexport) void __stdcall RVExtension(
    char *output,
    int output_size,
    const char *function
)
{
    ctab_web_rv_extension_impl(output, output_size, function);
}

__declspec(dllexport) int __stdcall RVExtensionArgs(
    char *output,
    int output_size,
    const char *function,
    const char **arguments,
    int argument_count
)
{
    return ctab_web_rv_extension_args_impl(
        output,
        output_size,
        function,
        arguments,
        argument_count
    );
}
