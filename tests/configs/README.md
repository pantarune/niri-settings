# Test configs

These files are fixtures for the core tests. They never touch the real
config.

| File | What it tests |
| --- | --- |
| `default.kdl` | A large config with every default niri option and bind |
| `noctalia/` | Shell-managed includes and the override file path |
| `dotfiles/` | Comments, an include in the middle, and values after it |
| `broken.kdl` | A parse error |
