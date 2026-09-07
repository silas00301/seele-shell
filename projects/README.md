# Projects

Each directory here owns one part of Seele. A project may contain runtime
sources and its Nix package definition. The root flake combines the projects.

The main shell package combines the agent, Bluetooth, audio, system, screen-link picker, and
Vicinae projects. The greeter, lock, and polkit projects build separately.
