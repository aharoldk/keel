# Variables and environments

Templates use `{{name}}` — whitespace inside the braces is tolerated, so `{{ id }}` works. Variables resolve in ordered scopes; later scopes win.

## Scopes and precedence

Lowest to highest:

1. **Workspace** — `.keel/workspace.yaml`, local, never committed
2. **Environment** — the selected file in `environments/` (its committed default, or the local current value when one is set)
3. **Collection** — `variables` in `collection.yaml`
4. **Folders** — `variables` in each `folder.yaml`, outer → inner
5. **Request** — `variables` on the request file itself
6. **Session** — values set by scripts during a run (in memory only)

A name that resolves nowhere is left in the output exactly as written (`{{missing}}`) and reported as missing — it never silently becomes empty. A higher scope that sets a name to an empty string does win and produces an empty value.

## Defaults vs current values

Every environment variable has two values:

- The **default** is committed in the environment file, next to the variable.
- The **current value** stays local: `.keel/env-values.yaml` for regular variables, the OS keychain for secrets.

At send time the current value wins; otherwise the default is used. `keel.setEnvVar(...)` in a script updates the current value of the active environment; setting it to an empty string clears it so the committed default wins again.

## Secrets

Secrets are declared in the active environment file as a map of name → committed default value:

```yaml
secrets:
  # The default is the committed fallback; the current value lives in the keychain.
  apiToken: local-dev-token
```

- The current value is stored in the OS keychain and always wins over the committed default. The committed default is only a fallback for a fresh clone.
- Secret values are kept out of history, logs, exports and reports. The CLI never prints them, and history stores URLs as templates.
- Scripts cannot read secret values: `keel.getVar("apiToken")` returns `null` for a secret name. Secrets only enter requests through `{{apiToken}}` interpolation into headers or URLs.
- Data-file columns cannot shadow a secret name — a row cell whose key matches a declared secret is dropped.

## Builtins

Names starting with `$` are builtins, available everywhere without declaring them:

| Builtin | Value |
| --- | --- |
| `{{$timestamp}}` | Milliseconds since the Unix epoch |
| `{{$uuid}}` | A random UUID v4 |
| `{{$randomInt}}` | A random integer below 1,000,000 |

## Chaining off the previous response

Inside a run, a request can read the response sent before it with `#{...}` tags. They resolve against the in-memory previous response before `{{name}}` runs, so no script or environment picking is needed.

| Tag | Meaning |
| --- | --- |
| `#{status}` | Previous response status code |
| `#{body}` | Previous response body, verbatim |
| `#{body.path}` | A scalar from the previous JSON body by dotted path (`#{body.data.items.0.id}` works for arrays) |
| `#{header.Name}` | A previous response header (case-insensitive) |

```yaml
request:
  method: GET
  url: "{{baseUrl}}/api/users/#{body.id}"
  headers:
    - name: Authorization
      value: "Bearer #{body.accessToken}"
```

Rules:

- Previous-response values are memory-only. They are replaced at send time and never written to disk.
- A tag that cannot be resolved (no previous response, missing path, non-JSON body with a path) is left exactly as written and reported as missing.
