# Format reference

The Keel format: every file the app writes, every field it reads. YAML keys are `camelCase`. Documents carry a `schemaVersion` (currently `"1"`); it is written on save and defaults to `"1"` when absent, so you can omit it by hand. Every document round-trips exactly, so the app never rewrites fields it does not understand into a different shape.

## Request files

A request is one YAML file. Example:

```yaml
schemaVersion: "1"
name: Create user
kind: request
description: raw body with language json → BodyType::Json.
request:
  method: POST
  url: "{{baseUrl}}/api/users"
  headers:
    - name: Content-Type
      value: application/json
  body:
    type: json
    content: |-
      {
        "name": "Ada",
        "email": "ada@keel.dev"
      }
auth:
  type: bearer
  token: "{{accessToken}}"
variables:
  userId: "1"
scripts:
  postResponse: |-
    test("created", () => expect(res.getStatus()).toBe(201));
tests:
  - expect: response.status
    toBe: 201
```

| Field | Type | Notes |
| --- | --- | --- |
| `schemaVersion` | string | Defaults to `"1"`. |
| `name` | string | Display name. The file name is the stable identity. |
| `kind` | string | Always `request`. Written on save; optional in hand-written files. |
| `description` | string | Free-form. |
| `protocol` | string | `http` (default), `graphql`, `websocket` or `grpc`. Informational for the editor; HTTP sends still go through `request`. |
| `request` | object | The HTTP request itself (see below). Required. |
| `auth` | object | Request-level auth; overrides inherited auth. See [Auth](#auth). |
| `variables` | map | Request-level variables — highest committed precedence. See [Variables](variables.md). |
| `scripts` | object | `preRequest` and `postResponse` sources. See [Scripting](scripts.md). |
| `tests` | list | Declarative assertions. See [Tests tab](tests.md). |
| `graphql` | object | Editor state for GraphQL: `operation`, `name`, `endpoint`. HTTP sends use a `graphql` body instead. |
| `websocket` | object | Editor state for WebSocket: `protocols` (comma-separated subprotocols). |
| `grpc` | object | Editor state for gRPC: `service`, `method`, `proto` (path to the `.proto` file). |

### The `request` block

```yaml
request:
  method: GET
  url: "{{baseUrl}}/api/users/:id"
  params:
    - name: limit
      value: "10"
  headers:
    - name: Accept
      value: application/json
      enabled: false
  pathParams:
    - name: id
      value: "{{userId}}"
  body:
    type: json
    content: '{"name": "Ada"}'
```

| Field | Type | Notes |
| --- | --- | --- |
| `method` | string | One of `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`, `TRACE`. |
| `url` | string | URL template. `{{name}}` placeholders are interpolated and `:name` segments replaced by `pathParams` at send time. |
| `params` | rows | Query parameters. |
| `headers` | rows | Request headers. |
| `pathParams` | rows | Values for `:name` segments in the URL. |
| `body` | object | Request body; see [Body types](#body-types). |

**Rows** (`params`, `headers`, `pathParams`) accept two forms — a list of objects:

```yaml
headers:
  - name: Accept
    value: application/json
```

or the concise map form:

```yaml
headers:
  Accept: application/json
```

The map form loses ordering and every row is enabled. Each row object:

| Field | Type | Notes |
| --- | --- | --- |
| `name` | string | Row name. |
| `value` | string | Row value. For multipart rows with `kind: file`, a path on disk. |
| `enabled` | bool | Default `true`. Disabled rows stay in the file but are skipped when sending. Serialized only when `false`, so diffs stay minimal. |
| `kind` | string | For multipart rows: `file` means `value` is a path on disk. |

### Body types

| `type` | Fields used | Content type sent |
| --- | --- | --- |
| `none` | — | no body |
| `json` | `content` | `application/json` |
| `text` | `content` | `text/plain` |
| `xml` | `content` | `application/xml` |
| `form-urlencoded` | `items` (rows) | `application/x-www-form-urlencoded` |
| `multipart` | `items` (rows) | `multipart/form-data` |
| `binary` | `path` | raw bytes read from `path` |
| `graphql` | `query`, `variables` (a JSON string) | `application/json` with the GraphQL envelope |

```yaml
body:
  type: multipart
  items:
    - name: file
      value: ./avatar.png
      kind: file
    - name: name
      value: Ada
```

```yaml
body:
  type: graphql
  query: |-
    query User($id: ID!) { user(id: $id) { name } }
  variables: '{"id": "1"}'
```

### Auth

```yaml
auth:
  type: bearer
  token: "{{accessToken}}"
```

| `type` | Fields |
| --- | --- |
| `none` | — |
| `bearer` | `token` — sent as `Authorization: Bearer <token>` |
| `basic` | `username`, `password` |
| `apikey` | `key`, `value`, `in` (`header` or `query`) |
| `digest` | `username`, `password` — retried once on a 401 challenge |
| `oauth2` | see below |

OAuth2 fields:

| Field | Notes |
| --- | --- |
| `grantType` | `clientCredentials`, `password` or `authorizationCode` (PKCE-capable) |
| `accessTokenUrl` | Token endpoint. |
| `refreshTokenUrl` | Refresh endpoint; defaults to the token endpoint. |
| `clientId`, `clientSecret` | Client credentials. |
| `scope` | Requested scope. |
| `authorizationUrl`, `callbackUrl` | Authorization endpoint and redirect URI (authorization code grant). |
| `pkce` | `true` to use PKCE. |
| `state` | Static `state` parameter. |
| `credentialsPlacement` | Where client credentials go: `basic` or `body`. |
| `tokenPlacement` | Where the access token goes: `header` or `query`. |
| `tokenHeaderPrefix` | Prefix for the header placement (e.g. `Bearer`). |
| `tokenQueryKey` | Query key for the query placement. |

Tokens live in memory and refresh when they expire. They are never written to disk.

## collection.yaml

The collection root, at the workspace root:

```yaml
schemaVersion: "1"
name: Demo API
description: A small Keel workspace.
variables:
  userId: "1"
defaultEnvironment: local
auth:
  type: bearer
  token: "{{apiToken}}"
headers:
  - name: X-Client
    value: keel-demo
scripts:
  preRequest: |-
    console.log('starting');
order:
  - users
  - auth
```

| Field | Notes |
| --- | --- |
| `name` | Collection name. |
| `description` | Free-form. |
| `variables` | Collection-wide variables. |
| `defaultEnvironment` | Environment file stem used when none is selected. |
| `auth` | Default auth for requests without their own; the nearest folder wins. |
| `headers` | Merged into every request; request and folder headers win. |
| `scripts` | Inherited by every request. |
| `order` | Child names (file or folder) in display order; missing names sort last. |

## folder.yaml

Optional, in any subdirectory:

```yaml
schemaVersion: "1"
kind: folder
name: Users
description: Everything about users.
variables:
  pageSize: "20"
auth:
  type: digest
  username: folder-user
  password: folder-pass
headers:
  - name: X-Folder
    value: users
scripts:
  postResponse: |-
    console.log('folder post');
order:
  - list-users.yaml
```

| Field | Notes |
| --- | --- |
| `kind` | `folder`. |
| `name` | Display label; the directory name is the identity. |
| `description` | Free-form. |
| `variables` | Folder-scoped variables. |
| `auth` | Folder auth; overrides outer folders and the collection. |
| `headers` | Merged into every request below the folder. |
| `scripts` | Folder scripts; see ordering below. |
| `order` | Child names in display order. |

### Inheritance rules

For a request at `users/admin/get-user.yaml`, the chain is `collection.yaml` → `users/folder.yaml` → `users/admin/folder.yaml` → the request.

- **Auth**: the nearest level that defines it wins — request first, then folders inner → outer, then the collection. A request without its own `auth` block gets the inherited one.
- **Headers**: all levels merge. Request rows win over folder rows, inner folders over outer, on a case-insensitive name match.
- **Pre-request scripts** run in order: collection → folders (outer → inner) → request.
- **Post-response scripts** run in reverse (sandwich): request → folders (inner → outer) → collection.
- **Variables** merge into ordered layers: workspace → environment → collection → folders (outer → inner) → request → session. See [Variables](variables.md).

## environments/*.yaml

One file per environment:

```yaml
schemaVersion: "1"
name: local
description: Local development server
variables:
  baseUrl: http://localhost:8080
  apiToken: local-dev-token
secrets:
  # Name → committed default value, used when the keychain holds no current value.
  apiToken: local-dev-token
```

| Field | Notes |
| --- | --- |
| `name` | Environment name, shown in the UI. |
| `description` | Free-form. |
| `variables` | Committed default values. |
| `secrets` | Declared secrets as a map of name → committed default value. The legacy list form (`secrets: [apiToken]`, default value empty) is still accepted. |

Current values for these variables are local overrides in `.keel/env-values.yaml` (regular variables) or the OS keychain (secrets). See [Variables and environments](variables.md).

## flows/*.yaml

A saved request sequence:

```yaml
schemaVersion: "1"
name: User lifecycle
kind: flow
steps:
  - auth/bearer-token.yaml
  - rest/create-user.yaml
  - path: rest/delete-user.yaml
    onFailure: continue
```

Steps are workspace-relative request paths, in run order. Each step is either a bare path string or an object with `path` and optional `onFailure`. Missing `onFailure` or anything other than `continue` stops the flow on failure.

## Reserved files and formats

- History records (`.keel/history.jsonl`) are one JSON object per line with `ts`, `method`, `url` (the unresolved template), `status`, `ok`, `timeMs`, `env` and `requestPath`.
- Runner data files are CSV (header row) or a JSON array of objects — column names become variable names for each iteration. Not Keel documents; never written as request YAML. See [Runs and flows](runs-and-flows.md).
