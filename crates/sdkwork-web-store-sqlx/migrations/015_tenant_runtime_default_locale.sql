-- 015: tenant runtime profile default locale overlay (I18N_SPEC / ENVIRONMENT_SPEC).
ALTER TABLE web_tenant_runtime_profile ADD COLUMN default_locale TEXT;
