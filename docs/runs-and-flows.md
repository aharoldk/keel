# Runs and flows

## Collection runs

A run sends requests one at a time and streams each result as it arrives. From the GUI you run a folder or a selection; from the CLI, `keel run <paths...>`.

Behavior:

- Folders recurse; `.git`, `.keel`, `environments`, `node_modules` and `target` are skipped.
- Requests run in collection order (`order` in `collection.yaml`/`folder.yaml`; missing names sort last).
- **Bail** (`--bail` in the CLI): stop at the first failed or errored request.
- **Delay** (`--delay MS`): wait between requests.
- A request passes when its tests all pass and the status is below 400; it fails when a test fails or the status is 4xx/5xx; it errors on a transport failure; scripts can mark it skipped.
- The previous response is kept in memory, so `#{body.token}` chaining works across a run.

Scripts control the run order:

```js
keel.setNextRequest("users/list-users.yaml");  // jump
keel.setNextRequest(null);                     // stop after this request
keel.skipRequest();                            // mark skipped
keel.stopExecution();                          // stop the run
```

## Data files

A run can iterate a data file — CSV with a header row, or a JSON array of objects. Column names become variables for that iteration, so the same request runs once per row:

```bash
keel run users/create-user.yaml --data rows.csv
```

Limits: 5 MB, 1000 rows. A column whose name matches a declared secret is ignored, so a data file cannot shadow the keychain. Without `--data`, every request runs exactly once.

## Flows

A flow is a saved request sequence in `flows/*.yaml`:

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

- Steps are workspace-relative request paths, run in order.
- A bare path string stops the flow if the request fails. `onFailure: continue` runs the next step anyway; anything other than `continue` stops.
- Run a flow from the sidebar row or the flow view. The flow view re-runs when finished.
- Import, duplicate and delete flows from the sidebar. A duplicate is a new uniquely named file, so nothing is overwritten.

## Reports

The CLI can write a JSON report with `--output report.json --format json`:

```json
{
  "items": [
    {
      "path": "users/create-user.yaml",
      "name": "Create user",
      "method": "POST",
      "url": "{{baseUrl}}/api/users",
      "status": "passed",
      "statusCode": 201,
      "timeMs": 12.5,
      "sizeBytes": 96,
      "testsPassed": 1,
      "testsTotal": 1,
      "error": null
    }
  ],
  "summary": { "total": 1, "passed": 1, "failed": 0, "errored": 0, "skipped": 0 }
}
```

The URL is always the unresolved template. Exit codes: `0` when everything passed, `1` on any failure or error, `2` on bad arguments.
