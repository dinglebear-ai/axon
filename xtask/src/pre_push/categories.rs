#[derive(Debug, Default)]
pub(super) struct Categories {
    pub(super) docs: bool,
    pub(super) workflow: bool,
    pub(super) rust: bool,
    pub(super) web: bool,
    pub(super) android: bool,
    pub(super) palette: bool,
    pub(super) chrome: bool,
    pub(super) docker: bool,
    pub(super) compose: bool,
    pub(super) mcp: bool,
    pub(super) security: bool,
    pub(super) release: bool,
    pub(super) version_files: bool,
    pub(super) openapi: bool,
    pub(super) codeql_actions: bool,
    pub(super) codeql_python: bool,
    pub(super) codeql_rust: bool,
}

impl Categories {
    pub(super) fn all() -> Self {
        Self {
            docs: true,
            workflow: true,
            rust: true,
            web: true,
            android: true,
            palette: true,
            chrome: true,
            docker: true,
            compose: true,
            mcp: true,
            security: true,
            release: true,
            version_files: true,
            openapi: true,
            codeql_actions: true,
            codeql_python: true,
            codeql_rust: true,
        }
    }

    pub(super) fn names(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        for (name, enabled) in [
            ("docs", self.docs),
            ("workflow", self.workflow),
            ("rust", self.rust),
            ("web", self.web),
            ("android", self.android),
            ("palette", self.palette),
            ("chrome", self.chrome),
            ("docker", self.docker),
            ("compose", self.compose),
            ("mcp", self.mcp),
            ("security", self.security),
            ("release", self.release),
            ("version_files", self.version_files),
            ("openapi", self.openapi),
            ("codeql_actions", self.codeql_actions),
            ("codeql_python", self.codeql_python),
            ("codeql_rust", self.codeql_rust),
        ] {
            if enabled {
                names.push(name);
            }
        }
        names
    }
}
