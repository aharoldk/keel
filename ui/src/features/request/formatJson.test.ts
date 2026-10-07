import { describe, expect, it } from "vitest";
import { formatJsonKeepingTags } from "./RequestEditor";

describe("formatJsonKeepingTags", () => {
  it("formats plain JSON unchanged", () => {
    expect(formatJsonKeepingTags('{"a":1}')).toBe('{\n  "a": 1\n}');
  });

  it("keeps a tag that is inside a string", () => {
    const out = formatJsonKeepingTags('{"password":"#{body.publicKey}"}');
    expect(out).toBe('{\n  "password": "#{body.publicKey}"\n}');
  });

  it("keeps a tag embedded in a larger string", () => {
    const out = formatJsonKeepingTags('{"auth":"Bearer #{body.token}"}');
    expect(out).toBe('{\n  "auth": "Bearer #{body.token}"\n}');
  });

  it("keeps a bare tag in a value position bare", () => {
    const out = formatJsonKeepingTags('{"password":#{body.publicKey},"n":1}');
    expect(out).toBe('{\n  "password": #{body.publicKey},\n  "n": 1\n}');
  });

  it("keeps {{var}} tags", () => {
    const out = formatJsonKeepingTags('{"id":"{{userId}}","bare":{{n}}}');
    expect(out).toBe('{\n  "id": "{{userId}}",\n  "bare": {{n}}\n}');
  });

  it("throws on genuinely broken JSON", () => {
    expect(() => formatJsonKeepingTags('{"a": }')).toThrow();
  });
});
