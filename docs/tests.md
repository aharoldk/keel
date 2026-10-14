# Tests tab

The Tests tab is separate from scripts: one assertion per row, not code. An assertion is an expression evaluated against the response plus exactly one matcher key:

```yaml
tests:
  - expect: response.status
    toBe: 200
  - expect: response.json.data.items
    toBeType: array
  - expect: response.headers.content-type
    toContain: application/json
```

## Expressions

Every expression starts with `response.`:

| Expression | Value |
| --- | --- |
| `response.status` | Status code |
| `response.time` | Round-trip time in ms |
| `response.size` | Response size in bytes |
| `response.body` | Response body, verbatim |
| `response.headers.<name>` | A response header (case-insensitive) |
| `response.cookies.<name>` | A response cookie |
| `response.json` | The parsed JSON body |
| `response.json.<path>` | A value from the JSON body by dotted path; array indexes work (`response.json.data.items.0.id`) |

## Matchers

| Matcher | Passes when |
| --- | --- |
| `toBe` / `toEqual` | Deep equality (numeric equality tolerates int/float) |
| `toNotBe` | Values are not deep-equal |
| `toContain` | String contains the value, array contains the element, or object has the key |
| `toMatch` | The stringified value matches the regex pattern |
| `toBeType` | The value's type matches: `null`, `boolean`, `number`, `string`, `array` or `object` |
| `toHaveLength` | Length equals the number (string, array or object) |
| `toBeGreaterThan` / `toBeLessThan` | Numeric comparison |
| `toBeTruthy` | The value is truthy |
| `toBeNull` | The value is null |

An assertion with no matcher passes when the expression is truthy.

Results appear per request (one row per assertion, with expected vs actual) and feed the pass/fail tally of collection runs and the CLI.
