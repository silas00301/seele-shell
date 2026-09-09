cmake_minimum_required(VERSION 3.21)
project(SeeleMarkdown LANGUAGES CXX)

find_package(Qt6 6.5 REQUIRED COMPONENTS Core Gui Qml Quick)
qt_standard_project_setup(REQUIRES 6.5)

# The plugin is the module: one self-contained library in the module directory,
# so the QML engine resolves it from QML_IMPORT_PATH without a second lookup.
qt_add_qml_module(seelemarkdown
  URI Seele.Markdown
  VERSION 1.0
  PLUGIN_TARGET seelemarkdown
  OUTPUT_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/Seele/Markdown
  SOURCES
    markdownhighlighter.cpp
    markdownhighlighter.h
)

target_link_libraries(seelemarkdown PRIVATE Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick)

install(DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/Seele
  DESTINATION lib/qt-6/qml
  FILES_MATCHING
    PATTERN "*.so"
    PATTERN "qmldir"
    PATTERN "*.qmltypes"
)
