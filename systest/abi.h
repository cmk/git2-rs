/*
 * ctest's alignment probe takes addresses of an uninitialized aggregate
 * containing git_rebase_operation.id (const), but never reads its value.
 * Keep this Clang warning visible without rejecting the generated probe;
 * retain every ABI check and treat all other warnings as errors.
 */
#if defined(__clang__)
# if __has_warning("-Wdefault-const-init-field-unsafe")
#  pragma clang diagnostic warning "-Wdefault-const-init-field-unsafe"
# endif
#endif
