//! Topic guard for the AI panel. The assistant is scoped to Keel's
//! domain — HTTP, gRPC, GraphQL and WebSocket requests, scripts, tests,
//! docs and commit messages — so it never answers off-topic questions
//! like "what's the weather?".
//!
//! Two layers, both cheap:
//!
//! 1. [`is_on_topic`] scans the prompt offline against a vocabulary of
//!    Keel terms. A prompt with no domain term is rejected before any
//!    network call.
//! 2. Every system prompt carries [`guard_instruction`], which tells the
//!    model to answer off-topic requests with the [`OFF_TOPIC`] sentinel.
//!    [`is_off_topic_response`] checks the reply for it, so prompts that
//!    slip past the vocabulary still get caught.

use std::collections::HashSet;
use std::sync::OnceLock;

/// The sentinel the model replies with when a request is off topic.
pub const OFF_TOPIC: &str = "OFF_TOPIC";

/// Error shown in the UI when a prompt or reply is out of scope.
pub const OFF_TOPIC_MESSAGE: &str =
    "Keel AI only helps with Keel workspaces: HTTP, gRPC, GraphQL, and WebSocket \
requests, scripts, tests, docs, and commit messages.";

/// Appended to every system prompt so the model rejects off-topic
/// requests itself with the [`OFF_TOPIC`] sentinel.
pub fn guard_instruction() -> &'static str {
    "You only answer questions about HTTP, gRPC, GraphQL, and WebSocket requests, \
Keel YAML files, scripts, tests, documentation, and git commit messages. \
If the user asks for anything else, reply with exactly `OFF_TOPIC` and nothing else."
}

/// Lowercase terms that mark a prompt as being about Keel's domain.
/// Matched per token, so phrases like "api key" or "status code" hit on
/// each word individually.
const VOCAB: &[&str] = &[
    // ---- HTTP ----
    "http", "https", "url", "uri", "endpoint", "endpoints", "api", "apis",
    "request", "requests", "response", "responses", "method", "methods",
    "get", "post", "put", "patch", "delete", "head", "options", "trace", "connect",
    "header", "headers", "param", "params", "parameter", "parameters",
    "query", "querystring", "body", "bodies", "payload", "payloads",
    "json", "xml", "form", "forms", "multipart", "urlencoded", "binary",
    "file", "files", "upload", "uploads", "download", "downloads",
    "rest", "restful", "soap",
    // ---- status codes ----
    "status", "statuscode", "2xx", "3xx", "4xx", "5xx",
    "200", "201", "202", "204", "206",
    "301", "302", "303", "304", "307", "308",
    "400", "401", "403", "404", "405", "406", "409", "410", "413", "415",
    "422", "429",
    "500", "501", "502", "503", "504",
    // ---- errors, retries, timeouts ----
    "error", "errors", "retry", "retries", "timeout", "timeouts",
    "backoff", "ratelimit",
    // ---- auth ----
    "auth", "authentication", "authorization", "authorize", "bearer", "basic",
    "digest", "oauth", "oauth2", "token", "tokens", "apikey", "jwt", "pkce",
    "credential", "credentials", "secret", "secrets", "keychain",
    "clientcredentials",
    // ---- tls, proxies, networking ----
    "tls", "ssl", "certificate", "certificates", "cert", "certs", "cacert",
    "clientcert", "proxy", "proxies", "redirect", "redirects", "cors", "origin",
    "host", "hosts", "hostname", "port", "ports", "route", "routes", "dns",
    "cname", "cdn", "loadbalancer", "http2", "http3",
    // ---- cookies and sessions ----
    "cookie", "cookies", "cookiejar", "jar", "session", "sessions",
    // ---- gRPC ----
    "grpc", "proto", "protos", "protobuf", "unary", "streaming", "stream",
    "streams", "metadata", "rpc", "servicemethod",
    // ---- GraphQL ----
    "graphql", "gql", "mutation", "mutations", "subscription", "subscriptions",
    "operation", "operations", "schema", "schemas", "introspection",
    // ---- WebSocket ----
    "websocket", "websockets", "ws", "wss", "socket", "sockets", "frame",
    "frames", "subprotocol", "subprotocols", "realtime",
    // ---- keel format ----
    "keel", "yaml", "yml", "doc", "docs", "document", "documents",
    "schemaversion", "folder", "folders", "collection", "collections",
    "flow", "flows", "step", "steps",
    // ---- scripts and tests ----
    "script", "scripts", "prerequest", "postrequest", "postresponse",
    "test", "tests", "testing", "assertion", "assertions", "assert", "asserts",
    "expect", "expects", "tobe", "toequal", "tocontain", "tomatch",
    "tobegreaterthan", "tobelessthan", "tobetruthy", "tobenull", "chai",
    "matcher", "matchers",
    // ---- variables, environments, chaining ----
    "variable", "variables", "var", "vars", "env", "envs", "environment",
    "environments", "builtin", "builtins", "timestamp", "uuid", "randomint",
    "interpolation", "placeholder", "placeholders", "chaining", "chain",
    // ---- import, export, codegen, cli ----
    "curl", "openapi", "swagger", "postman", "import", "imports", "export",
    "exports", "codegen", "snippet", "snippets", "fetch", "axios", "python",
    "httpie", "java", "golang", "node", "cli", "junit", "report", "reports",
    // ---- git ----
    "git", "commit", "commits", "committing", "diff", "diffs", "staged",
    "unstaged", "branch", "branches", "merge", "pull", "push", "rebase",
    "conflict", "conflicts", "oid",
    // ---- generic task verbs, so natural phrasings pass the fast path;
    // the OFF_TOPIC sentinel is the semantic backstop ----
    "add", "write", "create", "generate", "make", "build", "update", "change",
    "edit", "modify", "fix", "improve", "rename", "remove", "enable", "disable",
    "describe", "document", "explain", "suggest", "review", "validate",
    "parse", "extract", "convert", "migrate", "handle", "support", "implement",
    "refactor", "clean", "format", "summarize", "reword", "tighten",
];

fn terms() -> &'static HashSet<&'static str> {
    static TERMS: OnceLock<HashSet<&'static str>> = OnceLock::new();
    TERMS.get_or_init(|| VOCAB.iter().copied().collect())
}

/// True when the prompt mentions anything in Keel's domain.
///
/// Tokens are split on non-alphanumeric boundaries and lowercased, so
/// `OAuth2` hits `oauth2` and `therapy` never hits `api`.
pub fn is_on_topic(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    let terms = terms();
    lower
        .split(|c: char| !c.is_alphanumeric())
        .any(|token| !token.is_empty() && terms.contains(token))
}

/// True when a provider reply is the [`OFF_TOPIC`] sentinel rather than
/// real content. Tolerates fences, quotes and markdown emphasis around
/// it, and any casing the model picks.
pub fn is_off_topic_response(text: &str) -> bool {
    let cleaned = text
        .trim()
        .trim_matches(|c: char| c == '`' || c == '"' || c == '\'' || c == '*')
        .trim()
        .to_ascii_uppercase();
    !cleaned.is_empty() && cleaned.starts_with(OFF_TOPIC)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_off_topic_prompts() {
        for prompt in [
            "what's the weather",
            "tell me a joke",
            "who won the game last night",
            "what is the capital of France",
            "help me with my homework",
        ] {
            assert!(!is_on_topic(prompt), "{prompt:?} should be off topic");
        }
    }

    #[test]
    fn verb_only_prompts_reach_the_sentinel() {
        // Generic verbs pass the fast path so legit prompts like
        // "add pagination support" are never blocked offline; the model
        // rejects the nonsense with the OFF_TOPIC sentinel instead.
        assert!(is_on_topic("add pagination support"));
        assert!(is_on_topic("write me a poem about the sea"));
        assert!(is_off_topic_response("OFF_TOPIC"));
    }

    #[test]
    fn accepts_keel_topics() {
        for prompt in [
            "add a bearer token header",
            "write a test for the login endpoint",
            "update the request yaml",
            "extract the access token from the response",
            "retry on a 429",
            "add a websocket frame handler",
            "connect over wss and send a ping",
            "write a graphql mutation for createUser",
            "add a grpc streaming call",
            "parse the proto file",
            "generate curl for this",
        ] {
            assert!(is_on_topic(prompt), "{prompt:?} should be on topic");
        }
    }

    #[test]
    fn matches_whole_tokens_only() {
        assert!(!is_on_topic("therapy"));
        assert!(is_on_topic("call the api"));
        assert!(is_on_topic("OAuth2 with PKCE"));
        assert!(is_on_topic("YAML"));
    }

    #[test]
    fn detects_the_sentinel() {
        assert!(is_off_topic_response("OFF_TOPIC"));
        assert!(is_off_topic_response("`OFF_TOPIC`"));
        assert!(is_off_topic_response("  OFF_TOPIC  "));
        assert!(is_off_topic_response("**OFF_TOPIC**"));
        assert!(is_off_topic_response("off_topic"));
        assert!(is_off_topic_response("OFF_TOPIC — that's out of scope"));
        assert!(!is_off_topic_response("log(status())"));
        assert!(!is_off_topic_response(""));
        assert!(!is_off_topic_response("   "));
    }
}
