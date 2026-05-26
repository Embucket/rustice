-- Provider-side draft listing for Rustice Native App.
--
-- This creates a non-published, non-submitted listing object that can be edited
-- later in Provider Studio or with ALTER LISTING before it is shared with a
-- specific consumer account.
--
-- Replace IFSMGKM.UIC40916 with the target consumer ORG.ACCOUNT identifier.
-- Out-of-organization consumers require provider account auto-fulfillment and
-- a Native App application package that is allowed by Snowflake to use EXTERNAL
-- distribution with Snowpark Container Services.

CREATE EXTERNAL LISTING IF NOT EXISTS RUSTICE_NATIVE_APP_PRIVATE
  APPLICATION PACKAGE RUSTICE_NATIVE_APP_PKG
  AS $$
title: "Rustice Native App"
subtitle: "Snowflake-compatible SQL endpoint on Snowpark Container Services"
description: "Rustice runs a Snowflake-compatible SQL endpoint inside Snowpark Container Services and can query Snowflake-managed Iceberg tables through Horizon/Snowflake REST Catalog using a consumer-approved secret reference."
listing_terms:
  type: "OFFLINE"
targets:
  accounts:
    - "IFSMGKM.UIC40916"
auto_fulfillment:
  refresh_type: SUB_DATABASE_WITH_REFERENCE_USAGE
usage_examples:
  - title: "Start Rustice service"
    description: "Configure external access, bind the Horizon credential secret reference, then start the Native App service."
    query: |
      CALL <app_name>.APP_PUBLIC.START_APP();
$$
  PUBLISH = FALSE
  REVIEW = FALSE
  COMMENT = 'Draft private listing for Rustice Native App SPCS testing';

ALTER LISTING RUSTICE_NATIVE_APP_PRIVATE
  AS $$
title: "Rustice Native App"
subtitle: "Snowflake-compatible SQL endpoint on Snowpark Container Services"
description: "Rustice runs a Snowflake-compatible SQL endpoint inside Snowpark Container Services and can query Snowflake-managed Iceberg tables through Horizon/Snowflake REST Catalog using a consumer-approved secret reference."
listing_terms:
  type: "OFFLINE"
targets:
  accounts:
    - "IFSMGKM.UIC40916"
auto_fulfillment:
  refresh_type: SUB_DATABASE_WITH_REFERENCE_USAGE
usage_examples:
  - title: "Start Rustice service"
    description: "Configure external access, bind the Horizon credential secret reference, then start the Native App service."
    query: |
      CALL <app_name>.APP_PUBLIC.START_APP();
$$
  PUBLISH = FALSE
  REVIEW = FALSE
  COMMENT = 'Draft private listing for Rustice Native App SPCS testing';

SHOW LISTINGS LIKE 'RUSTICE_NATIVE_APP_PRIVATE';
