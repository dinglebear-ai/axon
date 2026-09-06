export type LabbyPaletteAction = {
  id: string;
  service: string;
  label: string;
  admin: boolean;
  destructive: boolean;
};

const services: Record<string, string> = {
  doctor:
    "access.check audit.full auth.check help oauth.relay.check proxy.check proxy.preflight schema system.checks",
  fs: "fs.list fs.preview help schema",
  gateway:
    "gateway.add gateway.client_config.get gateway.clients.list gateway.code_mode.get gateway.code_mode.set gateway.discover gateway.discovered_prompts gateway.discovered_resources gateway.discovered_tools gateway.enrich.apply gateway.enrich.preview gateway.get gateway.import gateway.import_pending.approve gateway.import_pending.list gateway.import_pending.reject gateway.import_tombstones.clear gateway.import_tombstones.list gateway.import_tombstones.restore gateway.list gateway.loadout.add gateway.loadout.get gateway.loadout.list gateway.loadout.list_state gateway.loadout.patch gateway.loadout.remove gateway.loadout.stage_patch gateway.loadout.stage_remove gateway.loadout.stage_update gateway.loadout.update gateway.mcp.cleanup gateway.mcp.disable gateway.mcp.enable gateway.mcp.list gateway.mcp.restart gateway.oauth.clear gateway.oauth.google_revoke gateway.oauth.probe gateway.oauth.resource_lease.create gateway.oauth.resource_lease.release gateway.oauth.resource_lease.renew gateway.oauth.start gateway.oauth.status gateway.oauth.wait gateway.protected_route.add gateway.protected_route.get gateway.protected_route.list gateway.protected_route.list_state gateway.protected_route.remove gateway.protected_route.stage_add gateway.protected_route.stage_remove gateway.protected_route.stage_update gateway.protected_route.test gateway.protected_route.update gateway.public_urls.get gateway.reload gateway.remove gateway.schema gateway.server.get gateway.servers gateway.service_actions gateway.service_config.get gateway.service_config.set gateway.skills.list gateway.status gateway.supported_services gateway.test gateway.update gateway.usage.calls gateway.usage.metrics gateway.virtual_server.disable gateway.virtual_server.enable gateway.virtual_server.get_mcp_policy gateway.virtual_server.quarantine.list gateway.virtual_server.quarantine.restore gateway.virtual_server.remove gateway.virtual_server.set_mcp_policy gateway.virtual_server.set_surface help schema",
  lab_admin: "help onboarding.audit schema",
  server_logs: "help schema server_logs.query",
  setup:
    "bootstrap check draft.commit draft.discard draft.get draft.set finalize help install_plugin installed_plugins plugin.install plugin.uninstall plugin_connectivity plugin_export plugin_hook plugin_sync plugins.installed proxy.configure repair schema schema.get services.status services_status settings.advanced_state settings.config.update settings.env.update settings.env_schema settings.schema settings.state settings.update state uninstall_plugin",
  skills: "help schema skills.get skills.list skills.read skills.search",
  snippets:
    "help schema snippets.create snippets.exec snippets.get snippets.list snippets.promote snippets.remove snippets.test snippets.validate",
};

const destructive = new Set([
  "gateway.oauth.google_revoke",
  "gateway.remove",
  "setup.bootstrap",
  "setup.draft.commit",
  "setup.draft.discard",
  "setup.draft.set",
  "setup.finalize",
  "setup.install_plugin",
  "setup.plugin.install",
  "setup.plugin.uninstall",
  "setup.plugin_hook",
  "setup.plugin_sync",
  "setup.proxy.configure",
  "setup.repair",
  "setup.settings.config.update",
  "setup.settings.env.update",
  "setup.settings.update",
  "setup.uninstall_plugin",
  "snippets.promote",
  "snippets.remove",
]);
const publicRead = new Set([
  "gateway.schema",
  "help",
  "schema",
  "doctor.access.check",
  "doctor.audit.full",
  "doctor.auth.check",
  "doctor.proxy.check",
  "doctor.proxy.preflight",
  "doctor.system.checks",
  "fs.fs.list",
  "fs.help",
  "fs.schema",
  "setup.check",
  "setup.help",
  "setup.schema",
  "setup.schema.get",
  "setup.settings.env_schema",
  "setup.settings.schema",
  "setup.settings.state",
  "setup.state",
  "skills.help",
  "skills.schema",
  "skills.skills.get",
  "skills.skills.list",
  "skills.skills.read",
  "skills.skills.search",
  "snippets.help",
  "snippets.schema",
  "snippets.snippets.list",
]);

function title(value: string): string {
  return (
    value
      .split(".")
      .at(-1)
      ?.replaceAll("_", " ")
      .replace(/\b\w/g, (letter) => letter.toUpperCase()) ?? value
  );
}

export const LABBY_PALETTE_ACTIONS: LabbyPaletteAction[] = Object.entries(services).flatMap(
  ([service, value]) =>
    value.split(" ").map((id) => {
      const qualified = id === "help" || id === "schema" ? `${service}.${id}` : `${service}.${id}`;
      return {
        id: qualified,
        service,
        label: title(id),
        admin: !publicRead.has(qualified) && !publicRead.has(id),
        destructive: destructive.has(id),
      };
    }),
);

export const LABBY_SERVICES = Object.keys(services);
