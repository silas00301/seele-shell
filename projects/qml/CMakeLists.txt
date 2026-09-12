cmake_minimum_required(VERSION 3.21)
project(SeeleCore LANGUAGES CXX)
find_package(Qt6 6.5 REQUIRED COMPONENTS Core Qml)
qt_standard_project_setup(REQUIRES 6.5)
find_library(SEELE_QML_CORE NAMES seele_qml_core REQUIRED)
find_path(SEELE_CORE_INCLUDE seele-core.h HINTS "${CMAKE_CURRENT_SOURCE_DIR}/../qml-core/include" REQUIRED)
qt_add_qml_module(seelecore
  URI Seele.Core
  VERSION 1.0
  PLUGIN_TARGET seelecore
  OUTPUT_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/Seele/Core
  SOURCES functions.cpp functions.h
)
target_include_directories(seelecore PRIVATE ${SEELE_CORE_INCLUDE})
target_link_libraries(seelecore PRIVATE Qt6::Core Qt6::Qml ${SEELE_QML_CORE} ${CMAKE_DL_LIBS})
install(DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/Seele
  DESTINATION lib/qt-6/qml
  FILES_MATCHING PATTERN "*.so" PATTERN "qmldir" PATTERN "*.qmltypes")

include(CTest)
if(BUILD_TESTING)
  set(SEELE_BOUNDARY_TEST "${CMAKE_CURRENT_SOURCE_DIR}/../../tests/native-functions-bounds.cpp" CACHE FILEPATH "Native QVariant boundary regression")
  add_executable(native-functions-bounds ${SEELE_BOUNDARY_TEST})
  target_include_directories(native-functions-bounds PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
  target_link_libraries(native-functions-bounds PRIVATE Qt6::Core)
  # The fixture deliberately uses assertions; keep them active in release builds.
  target_compile_options(native-functions-bounds PRIVATE -UNDEBUG)
  add_test(NAME native-functions-bounds COMMAND native-functions-bounds)
endif()
