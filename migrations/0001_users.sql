CREATE TABLE IF NOT EXISTS users (
    id uuid PRIMARY KEY NOT NULL,
    name text NOT NULL,
    email text NOT NULL UNIQUE,
    email_verified boolean DEFAULT false NOT NULL,
    image text,
    two_factor_enabled boolean DEFAULT false NOT NULL,
    role text,
    banned boolean DEFAULT false NOT NULL,
    ban_reason text,
    ban_expires timestamp,
    created_at timestamp DEFAULT now() NOT NULL,
    updated_at timestamp DEFAULT now() NOT NULL
);

CREATE TABLE IF NOT EXISTS accounts (
    id uuid PRIMARY KEY NOT NULL,
    account_id text NOT NULL,
    provider_id text NOT NULL,
    user_id uuid NOT NULL,
    password text,
    access_token text,
    refresh_token text,
    id_token text,
    scope text,
    created_at timestamp DEFAULT now() NOT NULL,
    updated_at timestamp DEFAULT now() NOT NULL,
    CONSTRAINT accounts_user_id_users_id_fk FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE cascade ON UPDATE no action
);
