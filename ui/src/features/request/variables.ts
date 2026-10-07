import { api } from "@/api/client";
import { useKeel } from "@/state/store";

/** Variable names available to a request, grouped by source (for hints). */
export interface VariableSuggestion {
  name: string;
  /** Where the value comes from. */
  source: "env" | "secret" | "collection" | "folder" | "prev";
  /**
   * Value that would be used right now. Env variables use the local current
   * value when set, otherwise the committed default. Secrets never include a
   * value — only the keychain holds the current one. Prev refs never include
   * a value either — a token in the last response must not leak into the UI.
   */
  value?: string;
}

const SOURCE_RANK: Record<VariableSuggestion["source"], number> = {
  collection: 0,
  folder: 1,
  secret: 2,
  env: 3,
  prev: 4,
};

/**
 * Collects every `{{name}}` the engine could resolve for a request:
 * active environment variables, its secret names, collection scope
 * variables, and each ancestor folder's scope variables. Later sources
 * win, matching inherit.rs precedence (env > folder > collection;
 * secrets are flagged so they render distinctly).
 */
export async function gatherVariableSuggestions(
  requestPath: string | null,
): Promise<VariableSuggestion[]> {
  const state = useKeel.getState();
  if (!state.workspace) return [];

  const merged = new Map<string, VariableSuggestion>();
  const put = (name: string, source: VariableSuggestion["source"], value?: string) => {
    const key = name.trim();
    if (!key) return;
    const prev = merged.get(key);
    if (!prev || SOURCE_RANK[source] > SOURCE_RANK[prev.source]) {
      merged.set(key, { name: key, source, value });
    }
  };

  const jobs: Promise<void>[] = [];

  // Collection scope variables (lowest precedence).
  jobs.push(
    api
      .collectionRead()
      .then((c) =>
        Object.entries(c.variables ?? {}).forEach(([k, v]) => put(k, "collection", v)),
      )
      .catch(() => {}),
  );

  // Ancestor folder scope variables (outer → inner).
  if (requestPath) {
    const parts = requestPath.split("/");
    for (let i = 1; i < parts.length - 1; i++) {
      const dir = parts.slice(0, i).join("/");
      jobs.push(
        api
          .folderRead(dir)
          .then((f) =>
            Object.entries(f?.variables ?? {}).forEach(([k, v]) => put(k, "folder", v)),
          )
          .catch(() => {}),
      );
    }
  }

  // Active environment variables + secret names.
  const activeEnv = state.activeEnv;
  if (activeEnv) {
    jobs.push(
      api
        .envRead(activeEnv)
        .then(async (doc) => {
          // Current values are local overrides and win over the committed
          // default, matching the engine's env resolution.
          const currents: Record<string, string> = await api
            .envValuesRead(activeEnv)
            .catch(() => ({}));
          Object.entries(doc.variables ?? {}).forEach(([k, v]) =>
            put(k, "env", currents[k] ?? v),
          );
          // Secret values stay in the keychain and are never shown here.
          Object.keys(doc.secrets ?? {}).forEach((k) => put(k, "secret"));
        })
        .catch(() => {}),
    );
  }

  await Promise.all(jobs);

  // Previous-response tags (`#{body.path}`, `#{header.Name}`, `#{status}`).
  // Paths only — values stay in the engine.
  jobs.push(
    api
      .prevRefs()
      .then((paths) => paths.forEach((p) => put(p, "prev")))
      .catch(() => {}),
  );

  await Promise.all(jobs);
  return [...merged.values()].sort((a, b) => a.name.localeCompare(b.name));
}
