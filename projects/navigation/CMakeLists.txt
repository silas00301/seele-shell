cmake_minimum_required(VERSION 3.21)
project(SeeleNavigation LANGUAGES CXX)
find_package(Qt6 6.5 REQUIRED COMPONENTS Quick Qml)
qt_standard_project_setup(REQUIRES 6.5)
qt_add_qml_module(seelenavigation
  URI Seele.Navigation VERSION 1.0 PLUGIN_TARGET seelenavigation
  OUTPUT_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/Seele/Navigation
  SOURCES navigator.cpp navigator.h)
target_link_libraries(seelenavigation PRIVATE Qt6::Quick Qt6::Qml)
install(DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/Seele DESTINATION lib/qt-6/qml
  FILES_MATCHING PATTERN "*.so" PATTERN "qmldir" PATTERN "*.qmltypes")
