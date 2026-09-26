/*
 * Keel scripting shim. Evaluated before user scripts; exposes the documented
 * globals (see docs/CONTRACT_V2.md "Scripting") on top of the native
 * __keel(op, payloadJson) host boundary.
 */
(() => {
  "use strict";

  const host = (op, payload) => {
    const out = __keel(op, JSON.stringify(payload || {}));
    let parsed;
    try {
      parsed = JSON.parse(out);
    } catch (e) {
      throw new Error("keel host returned malformed JSON");
    }
    if (parsed && parsed.error) throw new Error(parsed.error);
    return parsed;
  };

  const display = (v) => {
    if (typeof v === "string") return JSON.stringify(v);
    try {
      return JSON.stringify(v);
    } catch (e) {
      return String(v);
    }
  };

  const fmtArgs = (args) =>
    args
      .map((a) => (typeof a === "string" ? a : display(a)))
      .join(" ");

  /* ---------------- console ---------------- */
  const console = {
    log: (...a) => host("log", { message: fmtArgs(a) }),
    info: (...a) => host("log", { message: fmtArgs(a) }),
    debug: (...a) => host("log", { message: fmtArgs(a) }),
    warn: (...a) => host("log", { message: "warn: " + fmtArgs(a) }),
    error: (...a) => host("log", { message: "error: " + fmtArgs(a) }),
  };

  /* ---------------- expect (chai-lite) ---------------- */
  const deepEqual = (a, b) => {
    if (a === b) return true;
    if (typeof a === "number" && typeof b === "number") {
      return a === b || Math.abs(a - b) < 1e-9;
    }
    if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) {
      return false;
    }
    if (Array.isArray(a) !== Array.isArray(b)) return false;
    if (Array.isArray(a)) {
      if (a.length !== b.length) return false;
      for (let i = 0; i < a.length; i++) if (!deepEqual(a[i], b[i])) return false;
      return true;
    }
    const ka = Object.keys(a), kb = Object.keys(b);
    if (ka.length !== kb.length) return false;
    for (const k of ka) if (!deepEqual(a[k], b[k])) return false;
    return true;
  };

  const assertFail = (message) => {
    const err = new Error("AssertionError: " + message);
    err.name = "AssertionError";
    throw err;
  };

  const buildExpect = (actual, neg) => {
    const self = {};
    const words = ["to", "be", "been", "is", "that", "which", "and", "has", "have", "with", "at", "of", "but", "does", "still", "also"];
    for (const w of words) {
      Object.defineProperty(self, w, { get: () => self });
    }
    Object.defineProperty(self, "not", { get: () => buildExpect(actual, !neg) });
    const check = (cond, makeMsg) => {
      if (neg === !!cond) {
        assertFail(makeMsg() + (neg ? " (negated)" : ""));
      }
    };
    self.toBe = (e) => check(Object.is(actual, e) || (typeof actual === "object" && actual !== null && deepEqual(actual, e)), () => `expected ${display(actual)} to be ${display(e)}`);
    self.equal = self.toBe;
    self.toEqual = (e) => check(deepEqual(actual, e), () => `expected ${display(actual)} to deep-equal ${display(e)}`);
    self.eql = self.toEqual;
    self.toStrictEqual = self.toEqual;
    self.toContain = (e) => {
      let ok = false;
      if (typeof actual === "string") ok = actual.includes(String(e));
      else if (Array.isArray(actual)) ok = actual.some((x) => deepEqual(x, e));
      else if (actual && typeof actual === "object") ok = Object.prototype.hasOwnProperty.call(actual, String(e));
      check(ok, () => `expected ${display(actual)} to contain ${display(e)}`);
    };
    self.include = self.toContain;
    self.toMatch = (pattern) => {
      const re = pattern instanceof RegExp ? pattern : new RegExp(String(pattern));
      check(re.test(String(actual ?? "")), () => `expected ${display(actual)} to match ${String(pattern)}`);
    };
    self.toStartWith = (s) => check(String(actual ?? "").startsWith(String(s)), () => `expected ${display(actual)} to start with ${display(s)}`);
    self.toEndWith = (s) => check(String(actual ?? "").endsWith(String(s)), () => `expected ${display(actual)} to end with ${display(s)}`);
    self.toBeGreaterThan = (n) => check(Number(actual) > Number(n), () => `expected ${display(actual)} > ${display(n)}`);
    self.toBeGreatedThanOrEqual = null;
    self.toBeGreaterThanOrEqual = (n) => check(Number(actual) >= Number(n), () => `expected ${display(actual)} >= ${display(n)}`);
    self.toBeLessThan = (n) => check(Number(actual) < Number(n), () => `expected ${display(actual)} < ${display(n)}`);
    self.toBeLessThanOrEqual = (n) => check(Number(actual) <= Number(n), () => `expected ${display(actual)} <= ${display(n)}`);
    self.toBeTruthy = () => check(!!actual, () => `expected ${display(actual)} to be truthy`);
    self.toBeFalsy = () => check(!actual, () => `expected ${display(actual)} to be falsy`);
    self.toBeNull = () => check(actual === null, () => `expected ${display(actual)} to be null`);
    self.toBeUndefined = () => check(actual === undefined, () => `expected ${display(actual)} to be undefined`);
    self.toBeDefined = () => check(actual !== undefined, () => `expected ${display(actual)} to be defined`);
    self.toHaveLength = (n) => check(actual != null && actual.length === n, () => `expected ${display(actual)} to have length ${n}`);
    self.lengthOf = self.toHaveLength;
    self.property = self.toHaveProperty = (path, value) => {
      const parts = String(path).split(".");
      let cur = actual;
      for (const p of parts) {
        if (cur == null || !(p in Object(cur))) {
          check(false, () => `expected ${display(actual)} to have property ${path}`);
          return self;
        }
        cur = cur[p];
      }
      if (value === undefined) check(true, () => "");
      else check(deepEqual(cur, value), () => `expected property ${path} of ${display(actual)} to equal ${display(value)}`);
    };
    self.toOneOf = (list) => check(Array.isArray(list) && list.some((x) => deepEqual(x, actual)), () => `expected ${display(actual)} to be one of ${display(list)}`);
    Object.defineProperty(self, "a", {
      get: () => (type) => check(typeof actual === String(type).toLowerCase() || (type === "array" && Array.isArray(actual)) || (type === "null" && actual === null), () => `expected ${display(actual)} to be a ${type}`),
    });
    self.toTypeOf = self.a;
    self.toBeTypeOf = self.a;
    self.an = self.a;
    Object.defineProperty(self, "empty", {
      get: () => {
        const ok =
          typeof actual === "string" || Array.isArray(actual)
            ? actual.length === 0
            : actual && typeof actual === "object"
              ? Object.keys(actual).length === 0
              : false;
        check(ok, () => `expected ${display(actual)} to be empty`);
        return self;
      },
    });
    return self;
  };

  const expect = (actual) => buildExpect(actual, false);

  /* ---------------- test() ---------------- */
  const test = (name, fn) => {
    if (typeof name !== "string" || typeof fn !== "function") {
      throw new Error("test(name, fn) expects a description and a callback");
    }
    try {
      const r = fn();
      if (r && typeof r.then === "function") {
        host("reportTest", { name, pass: false, message: "async tests are not supported" });
      } else {
        host("reportTest", { name, pass: true, message: null });
      }
    } catch (e) {
      host("reportTest", { name, pass: false, message: String((e && e.message) || e) });
    }
  };

  /* ---------------- keel (bru is a compatibility alias) ---------------- */
  const call = (op, key, value) => host(op, value === undefined ? { [key]: null } : { [key]: value }).value;

  const keel = {
    getVar: (name) => call("getVar", "name", name) ?? null,
    setVar: (name, value) => host("setVar", { name, value: value === undefined ? null : value }),
    hasVar: (name) => !!call("hasVar", "name", name),
    deleteVar: (name) => host("deleteVar", { name }),
    getAllVars: () => call("getAllVars") ?? {},
    getEnvVar: (name) => call("getVar", "name", name) ?? null,
    setEnvVar: (name, value) => host("setEnvVar", { name, value: value === undefined ? null : value }),
    deleteEnvVar: (name) => host("deleteEnvVar", { name }),
    getCollectionVar: (name) => call("getCollectionVar", "name", name) ?? null,
    getFolderVar: (name) => call("getFolderVar", "name", name) ?? null,
    getEnvName: () => call("getEnvName") ?? "",
    interpolate: (v) => call("interpolate", "value", v),
    setNextRequest: (name) => host("setNextRequest", { name: name === undefined ? null : name }),
    skipRequest: () => host("skipRequest", {}),
    stopExecution: () => host("stopExecution", {}),
    sleep: (ms) => host("sleep", { ms: Number(ms) || 0 }),
    getTestResults: () => ({ summary: { total: 0, passed: 0, failed: 0 }, results: [] }),
    runner: {
      setNextRequest: (name) => host("setNextRequest", { name: name === undefined ? null : name }),
      skipRequest: () => host("skipRequest", {}),
      stopExecution: () => host("stopExecution", {}),
    },
  };

  /* ---------------- req / res ---------------- */
  const req = {
    getUrl: () => call("reqGet", "field", "url") ?? "",
    setUrl: (u) => host("reqSet", { field: "url", value: u }),
    getMethod: () => call("reqGet", "field", "method") ?? "GET",
    setMethod: (m) => host("reqSet", { field: "method", value: m }),
    getHeaders: () => {
      const rows = call("reqGet", "field", "headers") ?? [];
      const out = {};
      for (const r of rows) out[r.name] = r.value;
      return out;
    },
    getHeader: (name) => call("reqGetHeader", "name", name) ?? null,
    setHeader: (name, value) => host("reqSetHeader", { name, value }),
    removeHeader: (name) => host("reqRemoveHeader", { name }),
    deleteHeader: (name) => host("reqRemoveHeader", { name }),
    getBody: () => call("reqGet", "field", "body") ?? null,
    setBody: (b) => host("reqSet", { field: "body", value: typeof b === "string" ? b : JSON.stringify(b) }),
  };
  for (const [key, op] of [["url", "url"], ["method", "method"]]) {
    Object.defineProperty(req, key, {
      get: () => call("reqGet", "field", op),
      set: (v) => host("reqSet", { field: op, value: v }),
    });
  }
  Object.defineProperty(req, "body", {
    get: () => call("reqGet", "field", "body"),
    set: (v) => host("reqSet", { field: "body", value: v }),
  });

  const res = {
    getStatus: () => call("resGet", "field", "status"),
    getStatusText: () => call("resGet", "field", "statusText") ?? "",
    getHeaders: () => {
      const rows = call("resGet", "field", "headers") ?? [];
      const out = {};
      for (const r of rows) out[r.name] = r.value;
      return out;
    },
    getHeader: (name) => call("resGetHeader", "name", name) ?? null,
    getBody: () => call("resGet", "field", "body") ?? null,
    setBody: (b) => host("resSetBody", { value: typeof b === "string" ? b : JSON.stringify(b) }),
    getResponseTime: () => call("resGet", "field", "time") ?? 0,
    getSize: () => call("resGet", "field", "size") ?? 0,
  };
  Object.defineProperty(res, "status", { get: () => call("resGet", "field", "status") });
  Object.defineProperty(res, "body", { get: () => call("resGet", "field", "body") });
  Object.defineProperty(res, "headers", { get: () => res.getHeaders() });
  Object.defineProperty(res, "responseTime", { get: () => call("resGet", "field", "time") ?? 0 });
  Object.defineProperty(res, "url", { get: () => "" });

  /* ---------------- v0 DSL compatibility ---------------- */
  const json = (path) => {
    const r = host("jsonPath", { path });
    return r.value === undefined ? null : r.value;
  };
  const status = () => call("resGet", "field", "status") ?? 0;
  const header = (name) => call("resGetHeader", "name", name) ?? null;
  const time = () => call("resGet", "field", "time") ?? 0;
  const size = () => call("resGet", "field", "size") ?? 0;
  const set = (name, value) => host("setVar", { name, value: value === undefined ? null : value });
  const get = (name) => {
    const r = call("getVar", "name", name);
    return r === null || r === undefined ? null : r;
  };
  const log = (...a) => host("log", { message: fmtArgs(a) });

  /* ---------------- pm (Postman sandbox) ---------------- */
  const pmLocals = {};
  const collectionLocals = {};
  const remember = (name, value) => {
    pmLocals[name] = value;
  };
  const recall = (name) => {
    if (Object.prototype.hasOwnProperty.call(pmLocals, name)) return pmLocals[name];
    return keel.getVar(name);
  };
  const writeVar = (name, value) => {
    remember(name, value);
    keel.setVar(name, value);
  };
  const forget = (name) => {
    delete pmLocals[name];
    keel.deleteVar(name);
  };
  const varBag = (collection) => ({
    get: (name) => {
      if (collection) {
        if (Object.prototype.hasOwnProperty.call(collectionLocals, name)) return collectionLocals[name];
        return keel.getCollectionVar(name);
      }
      return recall(name);
    },
    set: (name, value) => {
      if (collection) collectionLocals[name] = value;
      writeVar(name, value);
    },
    unset: (name) => {
      if (collection) delete collectionLocals[name];
      forget(name);
    },
    has: (name) => {
      const v = collection
        ? Object.prototype.hasOwnProperty.call(collectionLocals, name)
          ? collectionLocals[name]
          : keel.getCollectionVar(name)
        : recall(name);
      return v !== null && v !== undefined;
    },
  });
  const pmVariables = varBag(false);
  pmVariables.replaceIn = (value) => keel.interpolate(value);

  const headerPair = (h) => {
    if (typeof h === "string") {
      const i = h.indexOf(":");
      if (i < 0) return null;
      return { name: h.slice(0, i).trim(), value: h.slice(i + 1).trim() };
    }
    if (h && typeof h === "object") return { name: h.key || h.name, value: h.value };
    return null;
  };
  const applyHeader = (h) => {
    const pair = headerPair(h);
    if (pair && pair.name) req.setHeader(pair.name, pair.value ?? "");
  };

  const responseChain = () => {
    const chain = {};
    for (const w of ["to", "be", "been", "is", "that", "and", "has", "have", "with"]) {
      Object.defineProperty(chain, w, { get: () => chain });
    }
    chain.status = (code) => {
      const actual = res.getStatus();
      if (Number(actual) !== Number(code)) {
        throw new Error(`expected status ${code} but got ${actual}`);
      }
    };
    chain.header = (name, value) => {
      const actual = res.getHeader(name);
      if (actual == null) throw new Error(`expected header ${name}`);
      if (value !== undefined && actual !== value) {
        throw new Error(`expected header ${name} to equal ${display(value)}`);
      }
    };
    chain.body = (expected) => {
      const actual = res.getBody();
      if (expected === undefined) {
        if (actual == null) throw new Error("expected a response body");
        return;
      }
      if (!deepEqual(actual, expected) && actual !== expected) {
        throw new Error(`expected body ${display(expected)} but got ${display(actual)}`);
      }
    };
    chain.jsonBody = (path, value) => {
      let data;
      try {
        data = JSON.parse(res.getBody() ?? "");
      } catch (e) {
        throw new Error("expected a JSON body");
      }
      if (path === undefined) return;
      let cur = data;
      for (const p of String(path).split(".")) {
        if (cur == null || !(p in Object(cur))) throw new Error(`expected JSON body to have ${path}`);
        cur = cur[p];
      }
      if (value !== undefined && !deepEqual(cur, value)) {
        throw new Error(`expected ${path} to equal ${display(value)}`);
      }
    };
    Object.defineProperty(chain, "ok", {
      get: () => {
        const code = Number(res.getStatus());
        if (!(code >= 200 && code < 300)) throw new Error(`expected 2xx status but got ${code}`);
        return chain;
      },
    });
    Object.defineProperty(chain, "json", {
      get: () => {
        JSON.parse(res.getBody() ?? "");
        return chain;
      },
    });
    return chain;
  };

  const pmResponse = {
    get code() {
      return res.getStatus();
    },
    get status() {
      return res.getStatusText();
    },
    get responseTime() {
      return res.getResponseTime();
    },
    get responseSize() {
      return res.getSize();
    },
    text: () => res.getBody() ?? "",
    json: () => {
      const body = res.getBody();
      if (body == null || body === "") return null;
      return JSON.parse(body);
    },
    headers: {
      get: (name) => res.getHeader(name),
      has: (name) => res.getHeader(name) != null,
    },
    get to() {
      return responseChain();
    },
  };

  const pmRequest = {
    get url() {
      const u = req.getUrl();
      return { toString: () => u, getRaw: () => u };
    },
    set url(v) {
      req.setUrl(String(v));
    },
    get method() {
      return req.getMethod();
    },
    set method(v) {
      req.setMethod(v);
    },
    headers: {
      add: applyHeader,
      upsert: applyHeader,
      remove: (name) => req.removeHeader(name),
      get: (name) => req.getHeader(name),
      has: (name) => req.getHeader(name) != null,
    },
  };

  const pm = {
    test,
    expect,
    environment: varBag(false),
    globals: varBag(false),
    variables: pmVariables,
    collectionVariables: varBag(true),
    request: pmRequest,
    get response() {
      return pmResponse;
    },
    sendRequest: () => {
      throw new Error("pm.sendRequest is not supported");
    },
    execution: {
      setNextRequest: (name) => keel.setNextRequest(name),
      skipRequest: () => keel.skipRequest(),
    },
  };

  const postman = {
    setEnvironmentVariable: (name, value) => writeVar(name, value),
    getEnvironmentVariable: (name) => recall(name),
    clearEnvironmentVariable: (name) => forget(name),
    setGlobalVariable: (name, value) => writeVar(name, value),
    getGlobalVariable: (name) => recall(name),
    clearGlobalVariable: (name) => forget(name),
    setNextRequest: (name) => keel.setNextRequest(name),
  };

  /* Postman test-script globals (`JSON.parse(responseBody)`, etc.). */
  Object.defineProperty(globalThis, "responseBody", {
    get: () => res.getBody() ?? "",
    configurable: true,
  });
  Object.defineProperty(globalThis, "responseCode", {
    get: () => ({ code: res.getStatus(), name: res.getStatusText(), detail: res.getStatusText() }),
    configurable: true,
  });
  Object.defineProperty(globalThis, "responseTime", {
    get: () => res.getResponseTime(),
    configurable: true,
  });

  /* ---------------- install ---------------- */
  globalThis.keel = keel;
  globalThis.bru = keel;
  globalThis.req = req;
  globalThis.res = res;
  globalThis.test = test;
  globalThis.expect = expect;
  globalThis.pm = pm;
  globalThis.postman = postman;
  globalThis.console = console;
  globalThis.json = json;
  globalThis.status = status;
  globalThis.header = header;
  globalThis.time = time;
  globalThis.size = size;
  globalThis.set = set;
  globalThis.get = get;
  globalThis.log = log;
  globalThis.__keelDone = true;
})();
