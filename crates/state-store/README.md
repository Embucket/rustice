# State Store

Utilities and models for persisting Embucket state into DynamoDB.

> Note: this crate is **not** currently a workspace member (it is absent from the root
> `Cargo.toml` `members` list); only `executor` tests reference it via the generated
> `MockStateStore`. Treat it as in-development / off the live request path.

## Models & API

`StateStore` (async trait, `#[mockall::automock]`) is implemented by `DynamoDbStateStore`
and covers three record types:

- `SessionRecord` — session id, TTL, session variables, views, timestamps.
- `ViewRecord` — `database.schema.name`, SQL definition, owner, TTL.
- `Query` — query id, request id, session id, SQL, start/end time, `ExecutionStatus`
  (`Running` / `Success` / `Fail` / `Incident`), and `QueryMetric`s.

## Single-table design

All three entity types share **one** DynamoDB table to keep related state co-located and
queryable in a single round trip:

- **Partition key (`PK`)** — `SESSION#{session_id}` for session/view records, `QUERY#{date}`
  for query records (so a day's queries share a partition).
- **Sort key (`SK`)** — the session id, or the query's `timestamp_millis`.
- **GSIs** — `GSI_QUERY_ID_INDEX`, `GSI_REQUEST_ID_INDEX`, `GSI_SESSION_ID_INDEX` allow
  looking queries up by id / request id / session id. These attributes are projected only
  onto query records.

Records are marshalled with `serde` + `serde_dynamo`; configuration comes from `STATESTORE_*`
and `AWS_DDB_*` environment variables (`config.rs`).

## Local DynamoDB setup

### Run DynamoDB Local

```bash
docker run -p 8000:8000 amazon/dynamodb-local -jar DynamoDBLocal.jar -sharedDb
```

Or (better with keys):

```bash
docker run -p 8000:8000 -e AWS_REGION=us-east-2 -e AWS_ACCESS_KEY_ID=local -e AWS_SECRET_ACCESS_KEY=local amazon/dynamodb-local -jar DynamoDBLocal.jar -sharedDb
```

### Create table

The state-store uses a single DynamoDB table for sessions, views, and queries. The
query-specific GSIs (`query_id`, `request_id`, `session_id`) are populated only on query records.

```bash
aws dynamodb create-table \
    --table-name embucket-statestore \
    --attribute-definitions \
        AttributeName=PK,AttributeType=S \
        AttributeName=SK,AttributeType=S \
        AttributeName=query_id,AttributeType=S \
        AttributeName=request_id,AttributeType=S \
        AttributeName=session_id,AttributeType=S \
    --key-schema AttributeName=PK,KeyType=HASH AttributeName=SK,KeyType=RANGE \
    --global-secondary-indexes \
        "IndexName=GSI_QUERY_ID_INDEX,KeySchema=[{AttributeName=query_id,KeyType=HASH}],Projection={ProjectionType=ALL},ProvisionedThroughput={ReadCapacityUnits=5,WriteCapacityUnits=5}" \
        "IndexName=GSI_REQUEST_ID_INDEX,KeySchema=[{AttributeName=request_id,KeyType=HASH}],Projection={ProjectionType=ALL},ProvisionedThroughput={ReadCapacityUnits=5,WriteCapacityUnits=5}" \
        "IndexName=GSI_SESSION_ID_INDEX,KeySchema=[{AttributeName=session_id,KeyType=HASH}],Projection={ProjectionType=ALL},ProvisionedThroughput={ReadCapacityUnits=5,WriteCapacityUnits=5}" \
    --provisioned-throughput ReadCapacityUnits=5,WriteCapacityUnits=5 \
    --endpoint-url http://localhost:8000 \
    --region us-east-2
```

Or create it manually at http://localhost:8001/.

### DynamoDB UI

Install:

```bash
npm install -g dynamodb-admin
```

Run after the container is started:

```bash
dynamodb-admin
```

Or (better with keys):

```bash
AWS_REGION=us-east-2 AWS_ACCESS_KEY_ID=local AWS_SECRET_ACCESS_KEY=local dynamodb-admin
```

### Environment variables

```bash
STATESTORE_TABLE_NAME=embucket-statestore
STATESTORE_DYNAMODB_ENDPOINT=http://localhost:8000
AWS_DDB_ACCESS_KEY_ID=key
AWS_DDB_SECRET_ACCESS_KEY=secret
# For temporary credentials
AWS_DDB_SESSION_TOKEN=token
```
