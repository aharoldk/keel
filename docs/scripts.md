# Scripting

Requests can carry two scripts:

```yaml
scripts:
  preRequest: |-
    // runs before the request is sent
  postResponse: |-
    // runs after the response arrives
```

Scripts run in a real JavaScript engine. Inherited scripts run too: pre-request collection → folders (outer → inner) → request; post-response request → folders (inner → outer) → collection.

Security model: scripts are local files you (or your Git history) authored. They can read plain variables but never secret values — `keel.getVar` on a secret name returns `null`.

## Globals

| Global | Available in | Purpose |
| --- | --- | --- |
| `keel` (alias `bru`) | both phases | Variables, environment, flow control |
| `req` | pre-request only | Read and modify the outgoing request |
| `res` | post-response only | Read the response; override the body |
| `test(name, fn)` | both | Register a test; failures inside `fn` fail the test |
| `expect(value)` | both | Chai-style assertions |
| `pm` | both | Postman sandbox compatibility |
| `postman` | both | Postman legacy function compatibility |
| `console` | both | `log`, `info`, `debug`, `warn`, `error` — collected as script logs |

## keel

```js
keel.getVar("baseUrl")            // read a variable (null when unset; null for secrets)
keel.hasVar("baseUrl")            // boolean
keel.setVar("userId", 42)         // set a session variable (any value; stored as string)
keel.deleteVar("userId")          // remove a session variable
keel.getAllVars()                 // all session variables as an object
keel.getEnvVar("baseUrl")         // same as getVar
keel.setEnvVar("token", "abc")    // set the current value of the active environment
keel.deleteEnvVar("token")        // clear the current value; the committed default wins
keel.getCollectionVar("apiVersion")
keel.getFolderVar("pageSize")
keel.getEnvName()                 // active environment display name
keel.interpolate("{{baseUrl}}/x") // best-effort {{name}} substitution (no secrets)
keel.setNextRequest("users/list-users.yaml")  // jump in a collection run
keel.setNextRequest(null)         // stop after the current request
keel.skipRequest()                // mark the current request skipped
keel.stopExecution()              // stop the whole run
keel.sleep(500)                   // sleep, clamped to 5000 ms
```

`keel.runner` exposes the same flow controls: `keel.runner.setNextRequest(name)`, `keel.runner.skipRequest()`, `keel.runner.stopExecution()`.

## req (pre-request only)

```js
req.getUrl()
req.setUrl("{{baseUrl}}/api/users")
req.getMethod()
req.setMethod("POST")
req.getHeaders()                  // { "Accept": "application/json" }
req.getHeader("Accept")
req.setHeader("X-Trace", "t1")    // adds or replaces, case-insensitive
req.removeHeader("Accept")
req.getBody()
req.setBody('{"name": "Ada"}')    // objects are JSON-stringified
```

`req.url`, `req.method` and `req.body` also work as properties.

## res (post-response only)

```js
res.getStatus()
res.getStatusText()
res.getHeaders()
res.getHeader("X-Trace")
res.getBody()
res.setBody('{"ok": true}')       // replaces the body used by later scripts/tests
res.getResponseTime()             // ms
res.getSize()                     // bytes
```

`res.status`, `res.body`, `res.headers` and `res.responseTime` also work as properties.

## test() and expect()

```js
test("status is 200", () => {
  expect(res.getStatus()).toBe(200);
});

test("body has users", () => {
  const json = JSON.parse(res.getBody());
  expect(json.users.length).to.be.above(0);
});
```

`expect()` is chai-style and supports chaining words (`to`, `be`, `been`, `is`, `that`, `which`, `and`, `has`, `have`, `with`, `at`, `of`, `but`, `does`, `still`, `also`) and `.not`:

| Matcher | Meaning |
| --- | --- |
| `toBe` / `equal` | strict or deep equality for objects |
| `toEqual` / `eql` / `toStrictEqual` | deep equality |
| `toContain` / `include` | substring, array member or object key |
| `toMatch` | regex (`expect(s).toMatch(/^a/)`) |
| `toStartWith` / `toEndWith` | string prefix/suffix |
| `toBeGreaterThan` / `toBeGreaterThanOrEqual` / `toBeLessThan` / `toBeLessThanOrEqual` | numeric comparison |
| `toBeTruthy` / `toBeFalsy` | truthiness |
| `toBeNull` / `toBeUndefined` / `toBeDefined` | null/undefined checks |
| `toHaveLength` / `lengthOf` | length equals n |
| `property("a.b"[, value])` | dotted path exists (and equals value when given) |
| `toBeTypeOf` / `a("string")` / `an("array")` | type check |
| `toBeOneOf([1, 2])` | member of a list |
| `empty` | empty string/array/object |

Notes:

- Async tests are not supported — a callback returning a promise reports as failed.
- An uncaught throw fails the enclosing test (or the whole script when thrown outside a `test` callback).

## pm.* (Postman compatibility)

Postman collections import with their scripts intact. The sandbox exposes:

```js
pm.test("status ok", () => pm.response.to.have.status(200));

pm.environment.set("token", pm.response.json().accessToken)  // persisted as the env current value
pm.collectionVariables.set("apiVersion", "v2")               // collection-scoped
pm.variables.get("baseUrl"); pm.variables.replaceIn("{{baseUrl}}/x")

pm.request.headers.upsert({ key: "X-Trace", value: "t1" })
pm.request.url; pm.request.method

pm.response.code              // status code
pm.response.responseTime      // ms
pm.response.text()            // body string
pm.response.json()            // parsed body
pm.response.headers.get("content-type")
pm.response.to.have.status(201)
pm.response.to.have.header("content-type")
pm.response.to.have.jsonBody("id", 7)
pm.response.to.be.ok          // 2xx
pm.response.to.be.json        // body parses as JSON
pm.expect(value).to.eql(other) // same expect engine

pm.execution.setNextRequest("users/list-users.yaml")
pm.execution.skipRequest()
```

Legacy `postman.*` functions are also available (`setEnvironmentVariable`, `getEnvironmentVariable`, `clearEnvironmentVariable`, `setGlobalVariable`, `getGlobalVariable`, `clearGlobalVariable`, `setNextRequest`), along with the `responseBody`, `responseCode` and `responseTime` globals.

`pm.sendRequest` is not supported and throws.

## v0 DSL compatibility

The original Keel script DSL works inside the JavaScript runtime:

| Function | Meaning |
| --- | --- |
| `set(name, value)` | set a session variable |
| `get(name)` | read a session variable (null when unset) |
| `log(...)` | collect a log line |
| `json("dot.path")` | value from the response JSON (post-response only) |
| `status()` | response status (post-response only) |
| `header("Name")` | response header (post-response only) |
| `time()` | response time in ms (post-response only) |
| `size()` | response size in bytes (post-response only) |

```js
set("userId", json("data.id"));
log("created user", get("userId"));
```
