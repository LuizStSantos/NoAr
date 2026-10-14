-- NoAr branding for the example sender address shown on the setup page.
UPDATE config_global
SET
    smtp_user = 'noar@example.org'
WHERE
    smtp_user = 'ffplayout@example.org';
