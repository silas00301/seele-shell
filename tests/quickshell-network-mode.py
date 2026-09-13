#!/usr/bin/env python3
"""Compile actual patched Quickshell mode conversion bodies with UBSan.

No network, Qt process, or system bus is used. Extract the upstream enum
constants and function bodies so this regression exercises the packaged patch,
including future source changes, instead of a separately maintained algorithm.
"""
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tempfile

source = Path(sys.argv[1])


def body(text, marker):
    start = text.index("{", text.index(marker))
    depth = 1
    end = start + 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end]


def enum(path, name):
    text = (source / path).read_text()
    text = text[text.index("class " + name + ":") :]
    match = re.search(r"enum Enum\s*:\s*quint8\s*\{.*?\};", text, re.S)
    if match is None:
        raise AssertionError("upstream enum shape changed: " + name)
    return "namespace " + name + " { " + match[0].replace("quint8", "std::uint8_t") + " }\n"


wire = body(
    (source / "src/network/nm/accesspoint.cpp").read_text(),
    "DBusDataTransform<qs::network::NM80211Mode::Enum>::fromWire(quint32 wire)",
)
mode = body(
    (source / "src/network/nm/wireless.cpp").read_text(),
    "auto translateMode = [this]()",
)
program = "#undef NDEBUG\n#include <cassert>\n#include <cstdint>\n#include <limits>\n"
program += "namespace qs::network {\n"
program += enum("src/network/nm/enums.hpp", "NM80211Mode")
program += enum("src/network/enums.hpp", "WifiDeviceMode")
program += "}\nusing namespace qs::network;\n"
program += "template<class T> struct DBusResult { T value; DBusResult(T v):value(v) {} };\n"
program += "auto fromWire(std::uint32_t wire) " + wire + "\n"
program += "struct Fixture { NM80211Mode::Enum input;\n"
program += "auto mode() const { return input; }\n"
program += "auto translate() { auto translateMode = [this]() " + mode
program += "; return translateMode(); } };\n"
program += r"""
int main() {
    const WifiDeviceMode::Enum expected[] = {
        WifiDeviceMode::Unknown, WifiDeviceMode::AdHoc, WifiDeviceMode::Station,
        WifiDeviceMode::AccessPoint, WifiDeviceMode::Mesh
    };
    // Every value representable by the enum must return a defined mode.
    for (unsigned value = 0; value <= 255; ++value) {
        const auto result = Fixture{static_cast<NM80211Mode::Enum>(value)}.translate();
        assert(result == (value < 5 ? expected[value] : WifiDeviceMode::Unknown));
    }
    // Wire values must be checked before narrowing, including values that
    // previously truncated into recognized Station/AP/etc enum values.
    const std::uint32_t wireValues[] = {
        0, 1, 2, 3, 4, 5, 254, 255, 256, 257, 258, 259, 260, 261,
        65535, 65536, 65537, 0x80000000U, 0xffffff01U, 0xffffffffU
    };
    for (auto value: wireValues) {
        const auto decoded = fromWire(value).value;
        assert(decoded == (value < 5 ? static_cast<NM80211Mode::Enum>(value)
                                    : NM80211Mode::Unknown));
        assert(Fixture{decoded}.translate() ==
               (value < 5 ? expected[value] : WifiDeviceMode::Unknown));
    }
}
"""
with tempfile.TemporaryDirectory(prefix="seele-quickshell-mode-") as temp:
    test = Path(temp) / "mode.cpp"
    executable = Path(temp) / "mode"
    test.write_text(program)
    subprocess.run(
        shlex.split(os.environ.get("CXX", "c++"))
        + ["-std=c++20", "-O2", "-Wall", "-Wextra", "-Werror=return-type",
           "-fsanitize=undefined", "-fno-sanitize-recover=all",
           str(test), "-o", str(executable)],
        check=True,
    )
    subprocess.run([str(executable)], check=True)
print("Quickshell: 256 enum values and 20 wire-boundary cases passed with UBSan")
