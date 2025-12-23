# Template plugin

This is a minimal uv project that tix can execute as a plugin.

## Usage
1) Copy this folder somewhere else.
2) Update the project name in `pyproject.toml`.
3) Implement `main(context, argv)` in `plugin.py`.
4) Register the plugin in `config.toml`.
