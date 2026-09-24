# Calculator

Open **Calculator** in the Control Center, **Seele Calculator** in Vicinae, or
run `seele-shellctl calculator` (also `seele-shellctl control calculator`). The
panel pins its output when opened and takes exclusive keyboard focus until
Escape, Close, another shell panel, or the same toggle dismisses it.

Type an expression for a live preview. Enter keeps it in the calculation tape
and leaves a fresh input; Ctrl+Enter copies the displayed result, including its
unit. Up/Down walks the tape and returns to the draft, Ctrl+L selects the input, and Tab reaches
the buttons. Each tape row can reuse its numerical result or copy its full
result. The tape contains at most 32 entries, newest first. Clearing the tape
also clears `ans`.

Arithmetic supports `+ - * / ^`, unary signs, parentheses, scientific notation,
`pi`, `e`, `ans`, `sqrt()`, `abs()` and `round()`. Power associates rightwards:
`2^3^2 = 512`, `-2^2 = -4`, and `2^-2 = 0.25`. Postfix `%` divides by 100, so
`240 * 15% = 36`; it is not a context-sensitive commercial percentage key.
Calculations use finite IEEE-754 double precision and display 12 significant
digits. `ans` retains the preceding unrounded numerical value; reusing a tape
row inserts the displayed rounded number. This is a general desktop calculator,
not an arbitrary-precision accounting system.

Conversions use `<expression> <unit> to <unit>`, for example `(2 + 3) ft to in`,
`72 F to C`, or `1 GiB to MB`. Supported dimensions and case-sensitive symbols:

| Dimension | Units |
| --- | --- |
| Length | mm cm m km in ft yd mi |
| Mass | mg g kg oz lb |
| Time | ms s min h day |
| Volume | mL L |
| Data | B kB MB GB TB KiB MiB GiB TiB |
| Temperature | C F K, with °C and °F aliases |

MB/GB are decimal and MiB/GiB are binary. Temperature conversion is affine and
rejects values below absolute zero. Units from different dimensions never
convert. `ans` after a conversion is the number in the destination unit;
include that unit explicitly in a subsequent conversion. Decimal input uses a
dot, and implicit multiplication and mixed-unit arithmetic are unsupported.

`projects/qml-core/src/calculator.rs` owns parsing, conversions, result/error
formatting and tape admission behind the shared in-process `Seele.Core` bridge.
The parser accepts at most 512 bytes with at most 32 levels of recursion.
`CalculatorPanel.qml` owns rendering and focus only. No expression executes
code, starts a process, contacts the network, or enters persistent history.
Clipboard copying is the only process handoff: the existing packaged `wl-copy`
receives the chosen text on stdin, never in arguments or shell code.

The shell instantiates the panel through a Loader only while it is visible.
Closing destroys the field (including its undo buffer), answer, tape and any
pending clipboard payload. An explicitly copied clipboard result remains on
the clipboard. Nothing is written to disk and no worker service is needed.
The new layer namespace is `seele-shell-calculator`; the parent compositor's
blur rule should include it.

Validation:

- `cargo test -p seele-qml-core --all-features --locked` exercises the production
  parser, conversions, bounds, invalid input and tape.
- `cargo clippy -p seele-qml-core --all-targets --all-features --locked -- -D warnings`
- `tests/calculator-interaction.sh` stages the actual panel and theme for
  `tests/tst_calculator.qml`, using the installed native plugin. It runs in the
  shell package install check and covers focus, real key input, preview,
  commit/recall, copy intent, invalid input, bounded history and destruction.
- The package installs and lints the panel; the Vicinae manifest is bundled by
  its existing command enumeration.
