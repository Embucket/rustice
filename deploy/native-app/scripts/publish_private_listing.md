# Private Listing Checklist

Use Provider Studio for the first private listing iteration.

1. Confirm the provider account is approved to publish Snowflake Native Apps
   with Snowpark Container Services.
2. Build and push the Rustice image to the provider Snowflake image repository.
3. Run `snow app run` from `deploy/native-app` and smoke-test the service.
4. Create a version/patch for the application package.
5. In Snowsight, open `Marketplace -> Provider Studio`.
6. Create a listing for `Specified Consumers`.
7. Select product type `Native App` and attach the Rustice application package.
8. Add each consumer organization/account identifier.
9. Add install/configuration instructions from `deploy/native-app/app/README.md`.
10. Publish the private listing.

Consumer validation:

```sql
SHOW PRIVILEGES IN APPLICATION <installed_app>;
SHOW REFERENCES IN APPLICATION <installed_app>;
CALL <installed_app>.APP_PUBLIC.CONFIGURE_EXTERNAL_ACCESS(...);
CALL <installed_app>.APP_PUBLIC.START_APP();
CALL <installed_app>.APP_PUBLIC.SERVICE_STATUS();
CALL <installed_app>.APP_PUBLIC.SERVICE_ENDPOINTS();
```
