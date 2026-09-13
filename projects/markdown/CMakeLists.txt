cmake_minimum_required(VERSION 3.21)
project(SeeleMarkdown LANGUAGES CXX)

find_package(Qt6 6.5 REQUIRED COMPONENTS Core Gui Qml Quick)
qt_standard_project_setup(REQUIRES 6.5)
find_library(SEELE_MARKDOWN_CORE NAMES seele_markdown_core REQUIRED)
find_package(PkgConfig REQUIRED)
pkg_check_modules(PCRE2 REQUIRED IMPORTED_TARGET libpcre2-8)

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

target_link_libraries(seelemarkdown PRIVATE Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick
  ${SEELE_MARKDOWN_CORE} PkgConfig::PCRE2 ${CMAKE_DL_LIBS})

install(DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/Seele
  DESTINATION lib/qt-6/qml
  FILES_MATCHING
    PATTERN "*.so"
    PATTERN "qmldir"
    PATTERN "*.qmltypes"
)

include(CTest)
if(BUILD_TESTING)
  find_package(Python3 REQUIRED COMPONENTS Interpreter)
  add_executable(markdown-formats tests/formats.cpp markdownhighlighter.cpp markdownhighlighter.h)
  target_include_directories(markdown-formats PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
  target_link_libraries(markdown-formats PRIVATE Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick
    ${SEELE_MARKDOWN_CORE} PkgConfig::PCRE2 ${CMAKE_DL_LIBS})
  add_test(NAME markdown-formats
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/tests/formats.py
      $<TARGET_FILE:markdown-formats> ${CMAKE_CURRENT_SOURCE_DIR}/tests/formats.json)
endif()
