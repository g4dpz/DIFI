# Regenerate the golden packets with the C++ tool into WORK_DIR, then byte-compare
# each against the committed shared oracle in GOLDEN_DIR. Any difference fails the test,
# proving the C++ wire format is byte-identical to the Rust-produced golden packets.

file(MAKE_DIRECTORY "${WORK_DIR}")

execute_process(
  COMMAND "${DIFI_TOOL}" gen-golden "${WORK_DIR}"
  RESULT_VARIABLE gen_rc
)
if(NOT gen_rc EQUAL 0)
  message(FATAL_ERROR "difi-tool gen-golden failed (rc=${gen_rc})")
endif()

set(files
  signal_context_16bit.difi.bin
  version_context.difi.bin
  signal_data_16bit.difi.bin
)

foreach(f ${files})
  execute_process(
    COMMAND ${CMAKE_COMMAND} -E compare_files "${WORK_DIR}/${f}" "${GOLDEN_DIR}/${f}"
    RESULT_VARIABLE cmp_rc
  )
  if(NOT cmp_rc EQUAL 0)
    message(FATAL_ERROR
      "golden mismatch: ${f} differs between C++ output and committed oracle")
  endif()
  message(STATUS "golden match: ${f}")
endforeach()

message(STATUS "All golden packets match the shared oracle.")
