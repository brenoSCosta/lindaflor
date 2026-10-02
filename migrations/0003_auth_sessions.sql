-- Auth sessions / verifications / two_factor for Topcoat.
-- sessions.token stores the hex-encoded SHA-256 TokenHash (not the raw session token).

CREATE TABLE IF NOT EXISTS sessions (
    id uuid PRIMARY KEY NOT NULL,
    expires_at timestamp NOT NULL,
    token text NOT NULL UNIQUE,
    created_at timestamp DEFAULT now() NOT NULL,
    updated_at timestamp DEFAULT now() NOT NULL,
    ip_address text,
    user_agent text,
    user_id uuid NOT NULL,
    impersonated_by uuid,
    CONSTRAINT sessions_user_id_users_id_fk FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE cascade ON UPDATE no action
);

CREATE TABLE IF NOT EXISTS verifications (
    id uuid PRIMARY KEY NOT NULL,
    identifier text NOT NULL,
    value text NOT NULL,
    expires_at timestamp NOT NULL,
    created_at timestamp DEFAULT now() NOT NULL,
    updated_at timestamp DEFAULT now() NOT NULL
);

CREATE TABLE IF NOT EXISTS two_factor (
    id uuid PRIMARY KEY NOT NULL,
    secret text NOT NULL,
    backup_codes text NOT NULL,
    user_id uuid NOT NULL,
    verified boolean DEFAULT false NOT NULL,
    CONSTRAINT two_factor_user_id_users_id_fk FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE cascade ON UPDATE no action
);

ALTER TABLE accounts ADD COLUMN IF NOT EXISTS access_token_expires_at timestamp;
ALTER TABLE accounts ADD COLUMN IF NOT EXISTS refresh_token_expires_at timestamp;

CREATE INDEX IF NOT EXISTS sessions_user_id_idx ON sessions (user_id);
CREATE INDEX IF NOT EXISTS verifications_identifier_idx ON verifications (identifier);
CREATE INDEX IF NOT EXISTS two_factor_secret_idx ON two_factor (secret);
CREATE INDEX IF NOT EXISTS two_factor_user_id_idx ON two_factor (user_id);
CREATE INDEX IF NOT EXISTS accounts_user_id_idx ON accounts (user_id);
