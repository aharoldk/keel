/**
 * Rewrites `#{…}` / `{{…}}` tags in JSON text, reporting whether each one
 * sits inside a JSON string (vs. in a value position). String state tracks
 * escapes so `\"` inside a value doesn't flip it.
 */
function mapJsonTags(text: string, f: (tag: string, inString: boolean) => string): string {
  const re = /#\{[^{}#]*\}|\{\{[^{}]*\}\}/g;
  let out = "";
  let last = 0;
  let inStr = false;
  let esc = false;
  const track = (s: string) => {
    for (const ch of s) {
      if (esc) {
        esc = false;
        continue;
      }
      if (ch === "\\" && inStr) esc = true;
      else if (ch === '"') inStr = !inStr;
    }
  };
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) {
    const before = text.slice(last, m.index);
    out += before;
    track(before);
    const emitted = f(m[0], inStr);
    out += emitted;
    track(emitted);
    last = m.index + m[0].length;
  }
  return out + text.slice(last);
}

/** Pretty-prints JSON while preserving `#{…}` / `{{…}}` tags, bare or not. */
export function formatJsonKeepingTags(content: string): string {
  const masks: { tag: string; outside: boolean }[] = [];
  const masked = mapJsonTags(content, (tag, inString) => {
    const token = `__KEEL_TAG_${masks.length}__`;
    masks.push({ tag, outside: !inString });
    // A bare tag sits in a value position — give the token quotes so the
    // masked text parses; inside a string the quotes are already there.
    return inString ? token : `"${token}"`;
  });
  const parsed = JSON.parse(masked);
  let out = JSON.stringify(parsed, null, 2);
  for (let i = 0; i < masks.length; i++) {
    const { tag, outside } = masks[i];
    const token = `__KEEL_TAG_${i}__`;
    out = outside
      ? out.replace(`"${token}"`, tag) // drop the JSON quotes again
      : out.split(token).join(tag); // substitute within the string
  }
  return out;
}
