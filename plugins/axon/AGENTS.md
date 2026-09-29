# Axon plugin contribution

The plugin is a client package for an existing Axon runtime, not the owner of the binary, providers, authentication implementation, or deployment lifecycle.

## Read before changing

[Plugin README](README.md) · [manifest](.claude-plugin/plugin.json) · [MCP configuration](../install-axon/.mcp.json) · [installation skill](../install-axon/skills/install-axon/SKILL.md)

[Setup reference](../../docs/reference/actions/setup.md) · [deployment](../../docs/operations/deployment.md) · [API authentication](../../docs/operations/auth/api-token.md) · [MCP connections](../../docs/reference/mcp/connect.md)

## Boundaries

Delegate installation/setup/deployment to the actual installer and setup/deploy surfaces. Do not embed a second provisioning system in a skill. User-invocable skills need agents/openai.yaml; keep their prompts, declarations, commands, and researcher agent consistent.

The plugin manifest has no version key and no automatic hooks. Repository .claude settings hooks are unrelated. Installing the plugin does not prove Axon is installed, reachable, authenticated, or deployed.

Use the configured server URL and caller's authorized credentials. Non-loopback access requires OAuth or static bearer; panel authorization remains separate. Do not claim a first-class Authelia integration from superficially compatible OIDC settings.

Setup instructions may change required env/config values, but must not overwrite unrelated values or secrets. Report exact failed probes and useful corrective actions rather than a generic installation failure.

## Validation

Run [plugin validation](../../scripts/validate_plugin.py), relevant setup/skill tests, and client discovery against the intended runtime for wire changes. Preserve release signature verification and existing configuration. Do not run production deployment merely to verify prose or copy private host details into shared skills.
