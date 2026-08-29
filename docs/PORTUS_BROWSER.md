# PortusBrowser Integration Baseline

**Last reviewed:** 2026-08-29T01:24:50Z
**Last updated:** 2026-08-29T01:24:50Z

**Status:** Authoritative PortusOS integration baseline
**Adopted:** 2026-07-27
**Source project:** https://github.com/PerceivingAI/portus-browser
**Scope:** What PortusBrowser provides, why it is required in PortusOS, what it owns, and how PortusOS integrates it

## 1. Decision

PortusBrowser is a required bundled component of PortusOS.

It is not an optional browser application and it is not merely a future integration target. It is one of the core capabilities that makes PortusOS agent-first.

Individual Chromium-based browsers remain selectable applications in the broader PortusOS model, but the first ISO has one explicit exception: Chromium is mandatory as the normal Codex ChatGPT authentication/default-browser surface and PortusBrowser reference target. Future profiles may offer Google Chrome and other supported Chromium-based browsers without changing PortusBrowser's required OS capability status.

## 2. What PortusBrowser is

PortusBrowser is a local-first browser-control subsystem that lets an AI agent and a user co-navigate one or more real, visible, user-approved Chromium-based browser sessions from the command line.

Its primary operating path is:

```text
agent
  -> portus-browser CLI
  -> local Portus Broker
  -> browser native-messaging host
  -> Portus Browser extension
  -> connected Chromium-based browser session
```

The current project is implemented as a TypeScript workspace containing:

- the `portus-browser` CLI used by agents;
- the local Portus Broker;
- the Chromium extension;
- the browser native-messaging host;
- a separate terminal native host for the extension terminal surface;
- shared protocol, transport, actions, snapshots, events, permissions, recipes, configuration, and logging packages;
- an agent skill that teaches agents how to operate the CLI safely.

## 3. Why it is required in PortusOS

Conventional computer-use systems normally give an agent screenshots and mouse control over one visible browser window.

PortusBrowser instead gives the agent a structured, code-first browser interface. It allows the agent to discover and target browsers, windows, tabs, pages, and elements directly while preserving a visual browser that the user can also see and use.

This supports the PortusOS operating model because the agent can:

- operate browsers without relying primarily on screen coordinates;
- control several connected browser sessions at once;
- maintain separate state for each browser and tab;
- use existing authenticated browser sessions without reading raw credentials;
- inspect page structure and actionable elements;
- perform browser workflows headlessly through the CLI while retaining visible rendering when useful;
- receive structured errors, events, state, and results;
- respect a browser-specific policy layer controlled by the owner.

PortusBrowser is therefore part of the required first-class agent capability set alongside Codex and the PortusOS control plane.

## 4. Current capabilities

PortusBrowser currently allows an agent to:

- list connected browser sessions;
- list and inspect tabs;
- open, navigate, activate, and close tabs;
- move backward and forward through tab history;
- wait for page load, URL, text, role, or element conditions;
- capture screenshots;
- capture structured page snapshots;
- click, type, fill forms, press keys, scroll, hover, and drag;
- dismiss page popups and cookie banners;
- handle native browser dialogs when the advanced backend is allowed;
- inspect captured console messages;
- inspect captured network requests;
- inspect permissions and effective browser policy;
- subscribe to live browser events;
- inspect recent events and retained session steps;
- discover and read saved browser recipes;
- work across multiple connected Chromium-based browsers and browser windows concurrently.

## 5. Structured snapshot model

PortusBrowser snapshots are a central agent-control mechanism.

A snapshot may contain:

- screenshot data;
- visible page text;
- viewport details;
- cleaned page information;
- structured elements;
- roles, labels, text, bounds, state, links, input metadata, and selector hints;
- a stable `snapshotId` for that captured page state;
- generated `elementId` values scoped to that snapshot.

The normal agent workflow is:

1. target a browser and tab;
2. capture or filter a snapshot;
3. select an element by structured attributes;
4. perform an action using the snapshot and element identifiers;
5. wait for or inspect the resulting state;
6. capture a new snapshot after the page changes.

Snapshots are invalidated when relevant page actions change the state. This prevents the agent from treating an element reference from an older page state as current.

Screenshots remain available for visual ambiguity and user-visible verification, but structured snapshots are the preferred browser-control path.

## 6. Multi-browser model

Every connected extension bridge registers a browser session with the Broker.

A session includes information such as:

- browser ID;
- browser type;
- extension version;
- connection and heartbeat state;
- reported capabilities;
- optional browser and profile labels;
- active policy and settings state.

The agent can target each browser separately and maintain separate working context for:

- `browserId`;
- `tabId`;
- current URL;
- current snapshot;
- element identifiers;
- events and retained steps.

PortusBrowser can therefore operate multiple browser types, windows, profiles, and tabs concurrently rather than exposing only one global browser session.

## 7. Broker responsibilities

The local Broker is the routing and coordination centre.

It owns or coordinates:

- browser-session registration;
- browser discovery;
- command routing;
- browser capability checks;
- heartbeats and stale-session expiry;
- command timeouts;
- structured protocol validation;
- local broker authentication;
- browser-domain policy enforcement before routing;
- settings-profile persistence and synchronisation;
- event publication and subscription;
- retained session-step history;
- recipe discovery;
- structured error normalisation.

The Broker communicates locally through a named pipe on Windows or a Unix socket on Linux and macOS. The protocol uses versioned request, response, and event envelopes.

## 8. Extension and native messaging

The browser extension performs browser-side work using the appropriate browser capabilities, including:

- extension APIs;
- content-script DOM operations;
- the optional debugger/CDP backend for advanced operations.

The native-messaging host relays versioned protocol messages between the extension and the local Broker.

The extension also provides user-facing popup, side-panel, settings, and terminal surfaces. These surfaces allow the user to connect the bridge, manage browser settings, inspect state, and use the optional built-in terminal.

PortusOS installs and integrates the local components, but it must not replace the browser extension's browser-domain responsibilities. For the first ISO the Broker lifecycle is provider-owned: the native host starts the Broker on demand when it is missing, and PortusOS must not add a competing OpenRC lifecycle owner.

## 9. Browser-domain policy

PortusBrowser owns its own browser policy layer.

Its policy includes:

- whether origin policy is enabled;
- allowlist or blocklist mode;
- allowed and blocked origins, including supported wildcard patterns;
- per-command enable or disable decisions;
- browser host permissions;
- advanced debugger availability;
- session-step retention;
- settings profiles and their selection by browser type.

PortusOS must not maintain a duplicate website whitelist or second browser-action policy.

PortusOS may impose an outer operating perimeter, such as whether PortusBrowser can run, which user/process identity it uses, what files it can access, whether it has general network access, and which tasks may invoke it. Once a request enters the PortusBrowser domain, its browser-specific policy applies.

A browser action must satisfy both applicable layers:

```text
PortusOS outer policy
  AND
PortusBrowser browser-domain policy
```

Neither layer silently overrides the other.

## 10. Settings profiles

PortusBrowser settings profiles preserve different browser security and terminal configurations without requiring the user to repeatedly replace individual values.

Profiles may contain:

- origin-policy settings;
- allow and block lists;
- command policy;
- retention settings;
- terminal settings;
- extension UX settings;
- auto-save behaviour.

The Broker owns saved profile content. Active profile selection is associated with browser type, so different browser types may use different profiles while multiple sessions of the same type share the selected profile.

PortusOS should treat this as PortusBrowser-owned domain state rather than copying it into the PortusOS policy registry.

## 11. Recipes

PortusBrowser recipes are saved workflow descriptions and context records.

They can express items such as:

- intent;
- desired state;
- constraints;
- saved context;
- output requirements;
- examples and notes.

Recipes do not directly operate the browser. The agent resolves or reads a recipe and then performs the workflow through normal PortusBrowser CLI commands.

## 12. Protected credentials and authenticated sessions

PortusBrowser's structured interface should allow Master Portus to use an authenticated browser session without providing a generic command that exports passwords, cookies, tokens, or browser credential databases.

However, under the PortusOS security model Master Portus is a broad non-sandboxed Linux user. Therefore omission of an export command is **not by itself** sufficient to claim that browser credentials are protected from Master Portus.

If the browser profile files or another same-UID browser interface allow Master Portus to copy reusable session/credential material directly, that material is user-readable in practice and must be classified accordingly.

A browser credential/session may be described as protected from Master Portus only when the OS/provider design prevents raw retrieval by the Master UID and the negative tests in `docs/SECRETS.md` pass.

The target pattern remains:

> Master Portus may use an authorised authenticated browser capability without receiving the underlying reusable secret value.

PortusBrowser remains responsible for browser-session and browser-permission behavior. PortusOS remains responsible for the outer Linux-user/policy boundary and protected-credential classification/enforcement.

## 13. PortusOS responsibilities

PortusOS must:

- bundle PortusBrowser in the supported installation;
- install the CLI, Broker, native host, extension assets, shared packages, skill, and required configuration;
- register native messaging for supported installed browsers;
- install and preserve the provider-owned native-host-on-demand Broker lifecycle without a competing OpenRC owner;
- make the Broker reliably available through that native-host startup path;
- expose PortusBrowser through capability discovery;
- report Broker and bridge health through Portus diagnostics;
- associate browser work with Portus tasks and requesting users;
- register downloaded or produced files as artifacts where applicable;
- preserve PortusBrowser's structured output and error model;
- integrate logs and high-level health without copying sensitive browser content indiscriminately;
- support repair or re-registration when a browser bridge or native host fails;
- validate operation after installation and updates.

PortusOS does not need to create one top-level OS tool for every PortusBrowser command. Under the common contract in `docs/CAPABILITIES.md`, the agent normally uses the `portus-browser` CLI directly; PortusOS owns provider registration, compatibility, health, task/resource relationships and recovery integration without becoming the browser-command proxy.

PortusBrowser version changes are governed at the OS level by `docs/UPDATES_RECOVERY.md`: the CLI, Broker, native host, extension assets/protocol compatibility, native-messaging registration and selected reference browser are treated as a cooperating compatibility set for a supported update rather than allowed to drift independently.

### 13.1 P15 first-ISO integration contract

The host-safe P15 implementation is now concrete:

```text
provider type        portus-browser
software version     0.1.0
protocol version     2
capability            browser.control
resource type         browser-session (session scoped, provider owned)
CLI                    /usr/bin/portus-browser
Broker                 /usr/bin/portus-broker
native host            /usr/bin/portus-native-host
native host name       com.portus.browser
Chromium host dir      <XDG_CONFIG_HOME>/chromium/NativeMessagingHosts
Broker lifecycle       provider-owned / native-host-on-demand
```

The machine-readable sources are `runtime/integrations/manifests/portus-browser.toml` and `runtime/integrations/portus-browser/integration.toml`. The integration contract validates the provider manifest, version/protocol compatibility, Chromium native-messaging destination and extension origin, bounded health/session projections and a deterministic repair/re-registration sequence.

The first-ISO source freeze is complete. `runtime/integrations/portus-browser/integration.toml` and `portusos-build/components/portus-browser.yaml` both lock clean immutable revision `c263c3997b4e6f2f7df5922e062a9e949e22f755` with `source_pin_state = "pinned"`. Installed Artix packaging, native-messaging registration, Chromium connection and ISO-23/24 behavior remain L5/VM evidence; the source revision itself is no longer unresolved.

Only bounded high-level Broker/session data crosses into generic Portus state. Browser sessions become principal-filtered opaque provider-resource references keyed by provider registration, `browser-session` resource ID and connection generation. Disappeared/replaced generations become stale rather than silently rebinding. Tabs, URLs, DOM, snapshots, page contents and browser credential/session stores remain PortusBrowser-owned.

P15 also adds the generic `provider_resource` System Index class and authoritative provider-registration relationship. Process/window/task correlations are added only when the installed Linux integration exposes reliable evidence; P15 does not guess them from browser IDs or labels.

## 14. PortusBrowser responsibilities

PortusBrowser owns:

- supported browser discovery and registration;
- browser IDs and session state;
- browser profiles and labels;
- windows and tabs;
- navigation;
- page snapshots and screenshots;
- element targeting;
- browser actions;
- page waits;
- console and network inspection;
- browser permissions;
- website and origin restrictions;
- browser command policy;
- browser settings profiles;
- browser-domain events and retained steps;
- browser recipe storage and resolution;
- browser bridge health and errors;
- browser credential/session behaviour.

## 15. Browser application choices

PortusBrowser is required. Individual browsers are separate choices.

PortusOS should support the browsers that PortusBrowser can operate, beginning with the Chromium family. Current implementation and protocol support includes or anticipates browser types such as:

- Chromium;
- Google Chrome;
- Microsoft Edge;
- Brave;
- other compatible Chromium-based browsers as explicitly supported.

The first-ISO reference validation profile includes Chromium. Future PortusOS profiles may select another supported browser or no preinstalled browser without changing PortusBrowser's required capability status.

The first-ISO supported baseline contains PortusBrowser plus Chromium. Beyond that first-ISO baseline, future PortusOS profiles may contain:

- PortusBrowser plus Google Chrome;
- PortusBrowser plus several supported browsers;
- PortusBrowser before a browser is installed, provided the system clearly reports that no compatible browser session is currently available.

## 16. Reliability and unattended operation

Because PortusOS may be physically unattended, the PortusBrowser integration must support:

- deterministic Broker startup;
- local-only transport by default;
- bridge heartbeat and stale-session detection;
- bounded command timeouts;
- clear unavailable, disconnected, permission, policy, timeout, target, and stale-snapshot errors;
- recovery after Broker restart;
- native-host re-registration or repair;
- browser/extension compatibility checks after updates under the managed update contract in `docs/UPDATES_RECOVERY.md`;
- health reporting when no browser is connected;
- operation without requiring a visible terminal window;
- task-safe handling when a browser or tab disappears during work.

A disconnected browser must produce a clear recoverable state rather than causing the agent to continue issuing blind actions.

## 17. First-ISO validation requirements

A PortusOS release should not claim PortusBrowser readiness until it validates at least:

- the `portus-browser` CLI is installed and returns structured output;
- the Broker starts and reports healthy status;
- local transport and broker-token behaviour work;
- the native host is installed and registered for the selected browser;
- the extension can connect its bridge;
- connected browsers are listed with stable IDs;
- tabs can be listed and inspected;
- a page can be opened and awaited;
- a structured snapshot can be captured;
- an element can be targeted and acted upon;
- screenshots work;
- policy correctly permits and blocks actions;
- browser-specific policy remains separate from PortusOS policy;
- session expiry and bridge-disconnect errors are reported correctly;
- downloaded outputs can be registered as Portus artifacts;
- the integration survives reboot and expected service restart;
- no visual terminal is required for normal agent operation.

## 18. Scope boundary

The PortusBrowser source project remains an independent specialised capability project.

The PortusOS repository owns:

- the decision to ship it;
- packaging and installation;
- OS-level configuration;
- lifecycle supervision;
- capability registration;
- task and artifact integration;
- health and recovery integration;
- release compatibility validation.

The PortusBrowser project owns its browser implementation and browser-domain contracts.

PortusOS must integrate the project rather than fork its responsibilities into the OS control plane.
