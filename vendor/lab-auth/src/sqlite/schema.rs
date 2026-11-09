// Preserve statement order for existing database bootstrap.
pub(super) const SCHEMA_SQL: &str = "CREATE TABLE IF NOT EXISTS registered_clients (
            client_id TEXT PRIMARY KEY,
            redirect_uris TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS authorization_requests (
            state TEXT PRIMARY KEY,
            client_id TEXT NOT NULL,
            redirect_uri TEXT NOT NULL,
            client_state TEXT NOT NULL,
            scope TEXT NOT NULL,
            provider_code_verifier TEXT NOT NULL,
            code_challenge TEXT NOT NULL,
            code_challenge_method TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS authorization_codes (
            code TEXT PRIMARY KEY,
            client_id TEXT NOT NULL,
            subject TEXT NOT NULL,
            redirect_uri TEXT NOT NULL,
            scope TEXT NOT NULL,
            code_challenge TEXT NOT NULL,
            code_challenge_method TEXT NOT NULL,
            provider_refresh_token TEXT,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS refresh_tokens (
            refresh_token TEXT PRIMARY KEY,
            client_id TEXT NOT NULL,
            subject TEXT NOT NULL,
            scope TEXT NOT NULL,
            provider_refresh_token TEXT,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS browser_sessions (
            session_id TEXT PRIMARY KEY,
            subject TEXT NOT NULL,
            email TEXT,
            csrf_token TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS browser_login_states (
            state TEXT PRIMARY KEY,
            return_to TEXT NOT NULL,
            provider_code_verifier TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS native_authorization_results (
            state TEXT PRIMARY KEY,
            code TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            expires_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS upstream_oauth_credentials (
            upstream_name             TEXT NOT NULL,
            subject                   TEXT NOT NULL,
            client_id                 TEXT NOT NULL,
            granted_scopes_json       TEXT NOT NULL,
            token_blob                BLOB NOT NULL,
            token_blob_nonce          BLOB NOT NULL,
            token_received_at         INTEGER NOT NULL,
            access_token_expires_at   INTEGER NOT NULL,
            refresh_token_present     INTEGER NOT NULL,
            PRIMARY KEY (upstream_name, subject)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS upstream_oauth_state (
            upstream_name   TEXT NOT NULL,
            subject         TEXT NOT NULL,
            csrf_token      TEXT NOT NULL,
            pkce_verifier   TEXT NOT NULL,
            created_at      INTEGER NOT NULL,
            expires_at      INTEGER NOT NULL,
            PRIMARY KEY (upstream_name, subject, csrf_token)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS upstream_oauth_dynamic_clients (
            upstream_name   TEXT NOT NULL,
            subject         TEXT NOT NULL,
            client_id       TEXT NOT NULL,
            created_at      INTEGER NOT NULL,
            PRIMARY KEY (upstream_name, subject)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS allowed_users (
            email       TEXT PRIMARY KEY NOT NULL,
            added_by    TEXT NOT NULL,
            created_at  INTEGER NOT NULL
        );";
