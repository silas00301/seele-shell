// Stable, allocation-owning C ABI. Parsing and source offsets belong to Rust;
// Qt supplies theme values and applies these operations to its text layout.
#pragma once
#include <cstddef>
#include <cstdint>

enum SeeleMarkdownFlags : std::uint32_t {
  SeeleInherit = 1, SeeleBold = 2, SeeleItalic = 4, SeeleStrike = 8,
  SeeleUnderline = 16, SeeleMono = 32, SeeleBackground = 64
};

extern "C" {
struct SeeleMarkdownSpan {
  std::uint32_t start, length, flags, color, heading;
};
struct SeeleMarkdownResult {
  SeeleMarkdownSpan *spans;
  std::size_t length;
  std::int32_t state;
};
SeeleMarkdownResult seele_markdown_highlight(const std::uint16_t *text, std::size_t length,
                                           std::int32_t previous, bool first);
void seele_markdown_free(SeeleMarkdownResult result);
}
