-- Development-only table used to simulate outbound email delivery.
-- NOTE: this stores the plaintext one-time code so the /dev/email-logs/latest
-- endpoint can surface it during local testing. The authoritative copy of the
-- code used for verification is stored hashed in login_challenges.code_hash.
-- This table must never be enabled/exposed in a production deployment.
CREATE TABLE IF NOT EXISTS email_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    to_email TEXT NOT NULL,
    subject TEXT NOT NULL,
    code TEXT NOT NULL,
    login_challenge_id UUID NOT NULL REFERENCES login_challenges(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_email_logs_to_email ON email_logs(to_email);