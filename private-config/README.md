# private-config

This directory is reserved for local, host-specific, or secret configuration.

Do not commit real runtime values here. Git only allows:

- `private-config/README.md`
- `private-config/.gitkeep`
- `private-config/*.example`
- `private-config/*.template`

Use `scripts/check_private_config_guard.sh` before commits and in CI to verify
that no private config file has entered the git index.
