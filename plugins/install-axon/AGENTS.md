# Axon installer plugin

Read the repository root AGENTS.md. This package owns `install-axon` and the Axon MCP client registration. Delegate installation and deployment to the canonical installer, setup, and deploy surfaces. Do not add automatic hooks, bundle a binary, overwrite unrelated host configuration, or claim a successful deployment from plugin installation alone. Run `python3 scripts/validate_plugin.py` and the focused installer contract test after changes.
