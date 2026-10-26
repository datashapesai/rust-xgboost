# Injected into XGBoost's CMake run through CMAKE_PROJECT_INCLUDE by build.rs
# (macOS builds with OpenMP only). CMake evaluates it right after project(),
# before any of XGBoost's own CMake files.
#
# XGBoost's patch_openmp_path_macos() attaches a POST_BUILD `install_name_tool`
# step to the `xgboost` target that rewrites the libomp path inside
# libxgboost.dylib. This crate only builds the static libxgboost.a, so that step
# fails with "can't open file: libxgboost.dylib" even though the archive was
# built correctly. XGBoost's sources are an unmodified upstream submodule that
# must not be patched, so instead we wrap add_custom_command and drop just that
# step. Every other add_custom_command call is forwarded unchanged to the
# builtin that CMake keeps available as _add_custom_command.
function(add_custom_command)
  if("${ARGN}" MATCHES "install_name_tool")
    message(STATUS "Skipping install_name_tool post-build step (static build)")
    return()
  endif()
  _add_custom_command(${ARGV})
endfunction()
