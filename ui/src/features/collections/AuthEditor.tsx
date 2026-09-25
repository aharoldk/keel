import React from "react";
import type { Auth, OAuth2Auth, OAuth2Grant } from "@/api/types";
import { Select } from "@/components/ui";
import VariableInput from "@/components/VariableInput";
import type { VariableSuggestion } from "@/features/request/variables";
import { cn } from "@/utils";

interface AuthEditorProps {
  auth: Auth | undefined;
  onChange: (a: Auth) => void;
  variables?: VariableSuggestion[];
}

const AUTH_TYPE_LABELS: Record<Auth["type"], string> = {
  none: "None",
  bearer: "Bearer",
  basic: "Basic",
  apikey: "API key",
  digest: "Digest",
  oauth2: "OAuth 2",
};

const GRANT_LABELS: Record<OAuth2Grant, string> = {
  clientCredentials: "Client Credentials",
  password: "Password",
  authorizationCode: "Authorization Code",
};

function skeleton(type: Auth["type"]): Auth {
  switch (type) {
    case "bearer":
      return { type: "bearer", token: "" };
    case "basic":
      return { type: "basic", username: "", password: "" };
    case "apikey":
      return { type: "apikey", key: "", value: "", in: "header" };
    case "digest":
      return { type: "digest", username: "", password: "" };
    case "oauth2":
      return {
        type: "oauth2",
        grantType: "clientCredentials",
        accessTokenUrl: "",
        clientId: "",
        clientSecret: "",
        credentialsPlacement: "body",
        tokenPlacement: "header",
        tokenHeaderPrefix: "Bearer",
        tokenQueryKey: "access_token",
      };
    default:
      return { type: "none" };
  }
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-center gap-2">
      <span className="w-40 shrink-0 text-xs text-fg-1">{label}</span>
      <div className="flex-1 min-w-0">{children}</div>
    </div>
  );
}

function ValueInput({
  value,
  onChange,
  placeholder,
  variables,
}: {
  value: string | undefined;
  onChange: (v: string) => void;
  placeholder?: string;
  variables?: VariableSuggestion[];
}) {
  return (
    <VariableInput
      className="w-full"
      inputClassName="font-mono"
      value={value ?? ""}
      placeholder={placeholder}
      onChange={onChange}
      variables={variables}
    />
  );
}

function CheckRow({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <label className="flex items-center gap-2 text-xs text-fg-1 cursor-pointer select-none">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className="h-3 w-3 shrink-0 accent-[var(--accent)] cursor-pointer"
      />
      {label}
    </label>
  );
}

export function AuthEditor({ auth, onChange, variables }: AuthEditorProps) {
  const current: Auth = auth ?? { type: "none" };
  const update = (patch: Record<string, unknown>) =>
    onChange({ ...current, ...patch } as Auth);

  return (
    <div className="flex flex-col gap-2">
      <Field label="Auth type">
        <Select
          className="w-full"
          value={current.type}
          onChange={(e) => onChange(skeleton(e.target.value as Auth["type"]))}
        >
          {(Object.keys(AUTH_TYPE_LABELS) as Auth["type"][]).map((t) => (
            <option key={t} value={t}>
              {AUTH_TYPE_LABELS[t]}
            </option>
          ))}
        </Select>
      </Field>

      {current.type === "none" && (
        <p className="pl-40 text-[10px] text-fg-2">
          No authentication will be sent for this scope.
        </p>
      )}

      {current.type === "bearer" && (
        <Field label="Token">
          <ValueInput
            value={current.token}
            onChange={(v) => update({ token: v })}
            placeholder="{{accessToken}}"
            variables={variables}
          />
        </Field>
      )}

      {current.type === "basic" && (
        <>
          <Field label="Username">
            <ValueInput
              value={current.username}
              onChange={(v) => update({ username: v })}
              placeholder="{{username}}"
              variables={variables}
            />
          </Field>
          <Field label="Password">
            <ValueInput
              value={current.password}
              onChange={(v) => update({ password: v })}
              placeholder="{{password}}"
              variables={variables}
            />
          </Field>
        </>
      )}

      {current.type === "apikey" && (
        <>
          <Field label="Key">
            <ValueInput
              value={current.key}
              onChange={(v) => update({ key: v })}
              placeholder="X-Api-Key"
              variables={variables}
            />
          </Field>
          <Field label="Value">
            <ValueInput
              value={current.value}
              onChange={(v) => update({ value: v })}
              placeholder="{{apiKey}}"
              variables={variables}
            />
          </Field>
          <Field label="Add to">
            <Select
              className="w-full"
              value={current.in ?? "header"}
              onChange={(e) =>
                update({ in: e.target.value as "header" | "query" })
              }
            >
              <option value="header">Header</option>
              <option value="query">Query params</option>
            </Select>
          </Field>
        </>
      )}

      {current.type === "digest" && (
        <>
          <Field label="Username">
            <ValueInput
              value={current.username}
              onChange={(v) => update({ username: v })}
              placeholder="{{username}}"
              variables={variables}
            />
          </Field>
          <Field label="Password">
            <ValueInput
              value={current.password}
              onChange={(v) => update({ password: v })}
              placeholder="{{password}}"
              variables={variables}
            />
          </Field>
        </>
      )}

      {current.type === "oauth2" && (
        <OAuth2Fields oa={current} update={update} variables={variables} />
      )}

      {current.type !== "none" && (
        <p className={cn("text-[10px] text-fg-2")}>
          Tip: reference variables with {"{{var}}"} — secrets belong in
          environments (stored in the keychain, never on disk).
        </p>
      )}
    </div>
  );
}

function OAuth2Fields({
  oa,
  update,
  variables,
}: {
  oa: OAuth2Auth;
  update: (patch: Record<string, unknown>) => void;
  variables?: VariableSuggestion[];
}) {
  const grant = oa.grantType ?? "clientCredentials";
  const placement = oa.tokenPlacement ?? "header";
  return (
    <>
      <Field label="Grant type">
        <Select
          className="w-full"
          value={grant}
          onChange={(e) => update({ grantType: e.target.value })}
        >
          {(Object.keys(GRANT_LABELS) as OAuth2Grant[]).map((g) => (
            <option key={g} value={g}>
              {GRANT_LABELS[g]}
            </option>
          ))}
        </Select>
      </Field>
      <Field label="Access token URL">
        <ValueInput
          value={oa.accessTokenUrl}
          onChange={(v) => update({ accessTokenUrl: v })}
          placeholder="{{baseUrl}}/oauth/token"
          variables={variables}
        />
      </Field>
      <Field label="Refresh token URL">
        <ValueInput
          value={oa.refreshTokenUrl}
          onChange={(v) => update({ refreshTokenUrl: v })}
          placeholder="{{baseUrl}}/oauth/token (optional)"
          variables={variables}
        />
      </Field>
      {grant === "authorizationCode" && (
        <>
          <Field label="Authorization URL">
            <ValueInput
              value={oa.authorizationUrl}
              onChange={(v) => update({ authorizationUrl: v })}
              placeholder="{{baseUrl}}/oauth/authorize"
              variables={variables}
            />
          </Field>
          <Field label="Callback URL">
            <ValueInput
              value={oa.callbackUrl}
              onChange={(v) => update({ callbackUrl: v })}
              placeholder="http://localhost:53682/callback"
              variables={variables}
            />
          </Field>
          <Field label="State">
            <ValueInput
              value={oa.state}
              onChange={(v) => update({ state: v })}
              placeholder="auto-generated when absent"
              variables={variables}
            />
          </Field>
          <Field label="">
            <CheckRow
              checked={oa.pkce ?? false}
              onChange={(v) => update({ pkce: v })}
              label="Use PKCE (S256)"
            />
          </Field>
        </>
      )}
      <Field label="Client ID">
        <ValueInput
          value={oa.clientId}
          onChange={(v) => update({ clientId: v })}
          placeholder="{{oauthClientId}}"
          variables={variables}
        />
      </Field>
      <Field label="Client secret">
        <ValueInput
          value={oa.clientSecret}
          onChange={(v) => update({ clientSecret: v })}
          placeholder="{{oauthClientSecret}}"
          variables={variables}
        />
      </Field>
      <Field label="Scope">
        <ValueInput
          value={oa.scope}
          onChange={(v) => update({ scope: v })}
          placeholder="read write"
          variables={variables}
        />
      </Field>
      {grant === "password" && (
        <>
          <Field label="Username">
            <ValueInput
              value={oa.username}
              onChange={(v) => update({ username: v })}
              placeholder="{{username}}"
              variables={variables}
            />
          </Field>
          <Field label="Password">
            <ValueInput
              value={oa.password}
              onChange={(v) => update({ password: v })}
              placeholder="{{password}}"
              variables={variables}
            />
          </Field>
        </>
      )}
      <Field label="Credentials in">
        <Select
          className="w-full"
          value={oa.credentialsPlacement ?? "body"}
          onChange={(e) =>
            update({
              credentialsPlacement: e.target.value as "body" | "basicAuthHeader",
            })
          }
        >
          <option value="body">Request body</option>
          <option value="basicAuthHeader">Basic auth header</option>
        </Select>
      </Field>
      <Field label="Token in">
        <Select
          className="w-full"
          value={placement}
          onChange={(e) =>
            update({ tokenPlacement: e.target.value as "header" | "query" })
          }
        >
          <option value="header">Header</option>
          <option value="query">Query param</option>
        </Select>
      </Field>
      {placement === "header" ? (
        <Field label="Token header prefix">
          <ValueInput
            value={oa.tokenHeaderPrefix}
            onChange={(v) => update({ tokenHeaderPrefix: v })}
            placeholder="Bearer"
            variables={variables}
          />
        </Field>
      ) : (
        <Field label="Token query key">
          <ValueInput
            value={oa.tokenQueryKey}
            onChange={(v) => update({ tokenQueryKey: v })}
            placeholder="access_token"
            variables={variables}
          />
        </Field>
      )}
    </>
  );
}

export default AuthEditor;
