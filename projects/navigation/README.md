# Keyboard navigation

Shell, Notes, lock, greeter and polkit instantiate one shared
`KeyboardNavigation` adapter. It routes keys within their own Qt windows and
uses the existing `FocusRing`; it never grabs desktop keys or moves the pointer.
Vicinae's extension views retain the launcher's native list and action-panel
keyboard handling.

| Key | Action |
| --- | --- |
| `h j k l` / arrows | Move in the drawn direction; a level or timeline takes Left/Right to adjust. |
| `Alt+h/j/k/l` | Move between controls, including out of an editor or level. |
| `Tab` / `Shift+Tab` | Walk controls in reading order. Editors retain their own Tab/indent behavior. |
| `gg` / `G` | First / last row in a list, or first / last control in a panel. |
| `Ctrl+d` / `Ctrl+u` | Move half a page through a list or scrollable outside text entry. |
| `/` | Focus the panel's search field. |
| `i` | Focus its first editable field. |
| `Enter` / `Space` | Activate the focused action; existing controls keep their own Space behavior. |
| `Menu` / `Shift+F10` | Secondary action on a custom target that accepts right-click. |
| `Shift+Enter` | Middle-click action when the target has one. |
| `Escape` / `q` | Leave the current view; Escape first gives an editor its own cancel/revert behavior, otherwise leaves text entry. |

Letters remain literal while editing, and Ctrl editing shortcuts remain intact.
Passwords, completions and draft values retain their own Escape handlers.
Hidden and disabled items are excluded from navigation. Lists keep activation
on the row navigation selected, including delegates created by scrolling.
The `g` prefix expires after 800 ms; held keys do not repeat activation.

`seele-shellctl bar` (Super+Ctrl+B on nerv) takes keyboard focus on the current
output's bar; Escape releases it. `p` on a Control Center module or bar module
toggles its bar placement. On a tray item, `p` toggles its hidden state,
Menu opens its menu, and `+`/`-` sends its scroll action. In the calendar, `y`
copies the focused date. Notes' sidebar divider adjusts with `h`/`l`, and its
playback track seeks five seconds at a time. Deliberately opened panels take
Wayland keyboard focus immediately; toasts, OSDs and other passive layers do not.

## Authoring

`ActionArea.qml` preserves MouseArea geometry, hover and drag behavior, and
shares `onTriggered` between pointer and keyboard activation. Set
`activateOnPress` for an action whose pointer timing is intentionally on press.
Use `onKeyPressed` for additional keys without replacing its activation handler.
If the parent already handles activation, keep one tab stop on that parent;
secondary actions still need an explicit key or a separate focusable target.
Continuous gestures need an explicit keyboard implementation on the owning
control; do not synthesize pointer coordinates to adjust a level or resize.

The C++ module contains Qt event, object and focus adaptation only. Scene
geometry, native control events and list model operations belong at this Qt
boundary; service and domain policy stay in Rust. The focus ring receives the
host's theme, so no second palette or material is introduced. Package the module
for every host of the shared components; auth clients depend on this small Qt
module without acquiring the Rust policy bridge.

## Checks

`nix build .#navigation` runs `tests/keyboard-navigation.sh` with real Qt key
events. Its fixture covers activation, geometry, literal text, leaving text
entry, popup focus boundaries, slider key release and virtualized lists with
page and endpoint movement. The production Control Center, calendar, themes
and Notes editor fixtures preserve their existing interaction contracts.
Build `default`, `notes`, `lock`, `greeter` and `polkit` after shared changes.
Finally check the bar shortcut, panel handoffs and focus release in Hyprland;
an offscreen Qt test cannot prove compositor focus or PAM/polkit behavior.
