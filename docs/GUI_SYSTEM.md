# PortusOS Graphical Session and Visual Fallback Contract

**Last reviewed:** 2026-08-29T01:24:50Z
**Last updated:** 2026-08-29T01:24:50Z

**Status:** Authoritative for the first-ISO graphical/session engineering baseline; the core Artix X11/i3/Alacritty/visual-fallback package set is locked, while session wiring and VMware/X11 execution remain pending
**Target:** x86_64 VMware-first PortusOS first ISO
**Acceptance authority:** `docs/ACCEPTANCE.md`
**Service ownership authority:** `docs/OPENRC_SERVICES.md`
**Policy authority:** `docs/POLICY.md`

## 1. Purpose and authority

This document owns the first-ISO X11/i3 session construction, graphical-session process ownership, presentation baseline, visual fallback, and graphical recovery boundary.

The first-ISO engineering direction is deliberately small:

> **TTY login -> authenticated user session -> conditional `startx` -> `dbus-run-session` -> i3 -> Alacritty / Portus presentation path.**

The graphical environment is a presentation, authentication, visual-application, monitoring and recovery surface. It is not the normal agent execution engine.

## 2. Session startup

The supported local Master graphical path is:

```text
Master Portus logs in on the designated local TTY
  -> PAM/elogind establish the authenticated login session
  -> XDG_RUNTIME_DIR and normal user-session environment exist
  -> conditional local-login rule invokes startx
  -> .xinitrc enters dbus-run-session
  -> small Portus session launcher starts required user-session helpers
  -> i3 starts
  -> Alacritty presents/attaches the MasterPortus tmux workflow
  -> portus-bootstrap / portus-master / Codex continue under their own contract
```

For the first ISO:

- no graphical display manager is required;
- SSH login must not invoke `startx` or create a graphical Master session;
- failure of `startx`, X11, i3 or Alacritty must leave TTY/SSH recovery available;
- X11/i3 and user-session helpers remain session-owned rather than OpenRC machine services.

The exact TTY predicate and shell/profile implementation must be verified on the Artix reference VM, but it must distinguish the intended local console login from SSH and unrelated TTY sessions.

## 3. D-Bus, elogind and user services

System D-Bus and elogind are machine prerequisites owned by OpenRC under `docs/OPENRC_SERVICES.md`.

The first ISO does **not** make OpenRC user-service orchestration a required dependency for the graphical session. OpenRC supports user services, but PortusOS initially uses the simpler authenticated-session path until the exact Artix user-service integration has been proven necessary and reliable.

The baseline is:

- PAM/elogind establish the login/session context and `XDG_RUNTIME_DIR`;
- `dbus-run-session` creates the user D-Bus session for the graphical login;
- PipeWire, WirePlumber and other intrinsically user-scoped helpers, when included, start from the Portus graphical-session launcher or another verified session-native mechanism;
- no session helper is promoted to a machine daemon merely for convenience.

## 4. i3 baseline

The first-ISO i3 configuration should remain intentionally small and deterministic.

Baseline rules:

- use `Mod4` as the primary modifier;
- retain standard i3 reload/restart/exit controls required for direct recovery;
- provide a direct Alacritty launch binding;
- require no compositor for first-ISO acceptance;
- require no VMware 3D acceleration;
- keep workspace names/numbers configuration-driven rather than embedding a large semantic desktop layout in code;
- preserve deterministic placement/focus through `i3-msg` / i3 IPC;
- do not make simulated keyboard/mouse input the normal application-routing mechanism.

The first presentation autostart should be equivalent in behavior to:

```text
Alacritty -> portus-bootstrap -> MasterPortus tmux attachment -> portus-master -> Codex
```

The exact command wrapper may differ if needed to avoid duplicate terminals or to resume an existing tmux session cleanly, but i3 does not own Master/Codex lifecycle semantics.

## 5. Workspaces and application placement

The first ISO needs only enough workspace policy to support deterministic presentation and System Index correlation.

Requirements:

- i3 IPC remains the authoritative workspace/window control path;
- Portus may discover and correlate windows/workspaces through the System Index;
- workspace policy is configuration-driven and can be changed without rebuilding i3 integration code;
- application launch/placement should prefer application commands plus i3 IPC over visual input;
- scratchpad/hidden state may be used, but no first-ISO feature depends on a complex scratchpad convention.

## 6. Audio and notifications

Audio is not allowed to distort the lifecycle architecture.

If audio is included in the first ISO:

- PipeWire and WirePlumber are user-session processes;
- they are not OpenRC machine daemons;
- failure of audio must not prevent the graphical session, SSH or Master Portus from operating.

A lightweight notification agent may be included when needed by the first-ISO user experience. It remains session-owned and optional to core recovery.

## 7. Polkit, keyring and authentication agents

Graphical polkit/keyring/authentication agents, where required by selected packages, are session-owned.

They must not silently become substitutes for the administrator/root and protected-secret boundaries already defined by `docs/POLICY.md` and `docs/SECRETS.md`.

The base `polkit` package is selected in the first-ISO graphical package set. Any additional graphical polkit/keyring/authentication agent and its startup mechanism remains capability-driven and must be verified before inclusion.

## 8. Visual fallback

Structured CLI/API/provider control remains preferred.

The first ISO nevertheless requires a real visual fallback consisting of:

- X11 screenshot capture;
- pointer control;
- keyboard control;
- observable resulting graphical state.

The first-ISO package contract now selects `maim` for screenshot capture, `xdotool` for pointer/keyboard fallback and `xorg-xprop` for X11 inspection alongside the core X11/i3 stack. The exact P14 backend command binding and resulting behavior remain to be exercised in VMware.

Visual input must remain policy-controlled, and sensitive credential-entry surfaces must not be captured, logged or replayed casually.

The host-safe P14 implementation materializes this boundary in the shared `portus-visual` crate. It consumes the existing System Index X11/i3 generation-scoped observations rather than building another desktop inventory, requires authenticated-principal ownership and exact target-generation revalidation, prefers structured/provider control, focuses then revalidates before keyboard/pointer input, blocks sensitive or unclassified screens, and prepares retained screenshots through the existing private/temporary task-artifact contract. The crate defines a narrow backend interface; exact screenshot/input commands remain an Artix package/VMware verification item and are not hard-coded into the architecture from Windows.

## 9. Sensitive-screen handling

Credential and AUTH surfaces must preserve the existing secret/policy boundary.

At minimum:

- reusable secrets must not be copied into ordinary Portus logs or task output;
- screenshot/debug capture around sensitive entry must be suppressible;
- visual fallback must not become an automated secret-extraction path;
- graphical auth failure must preserve non-graphical administrator recovery.

## 10. VMware graphics boundary

The first ISO proves X11+i3 in the reference VMware VM **without depending on 3D acceleration**.

Engineering order:

1. rely on kernel/native VMware virtual-device support and the ordinary Xorg/modesetting path first;
2. verify resolution/input behavior in the reference VM;
3. add a dedicated VMware X driver only if current Artix packages and measured behavior show it is required or materially improves the supported first-ISO path;
4. do not add a non-Artix graphics component merely for cosmetic improvement.

`open-vm-tools` is governed separately by `docs/HARDWARE.md` and `docs/PACKAGE_POLICY.md` because its current Artix availability and necessity must be verified before inclusion.

## 11. Recovery boundary

If the graphical session fails:

- the local TTY remains usable;
- SSH remains usable once encrypted boot/networking have completed;
- OpenRC machine services remain independently manageable;
- Master Portus startup can be diagnosed without requiring X11;
- root/admin recovery remains independent;
- the live ISO remains the low-level repair route.

No first-ISO acceptance claim may require the GUI to repair the GUI.

## 12. Remaining Linux verification

Still to verify/freeze on the Artix reference VM:

- live/installed behavior of the locked `xorg-server`, `xorg-xinit`, `xorg-xprop`, `i3-wm`, `alacritty`, `mesa`, `maim`, `xdotool` and `polkit` package set;
- exact PAM/elogind configuration that establishes the desired session;
- exact local-TTY conditional `startx` behavior;
- exact `dbus-run-session` / `.xinitrc` / Portus session-launcher behavior;
- exact PipeWire/WirePlumber package/startup behavior if audio is retained;
- exact additional notification/polkit/keyring agents required by measured first-ISO needs;
- P14 backend binding and VMware proof for the selected screenshot/pointer/keyboard fallback tools;
- exact VMware display/input behavior and whether any dedicated X driver is justified;
- exact first-ISO workspace names/defaults and sensitive-screen controls.

## 13. First-ISO acceptance implications

Validation must prove at minimum that:

- the intended local Master login reaches X11+i3 through the conditional `startx` path;
- SSH login does not start the GUI;
- system D-Bus/elogind prerequisites work without converting graphical helpers into machine services;
- i3 starts without requiring VMware 3D acceleration;
- Alacritty can reach the intended MasterPortus presentation path;
- screenshot and pointer/keyboard fallback work under policy;
- stopping or breaking X11/i3 leaves TTY/SSH/non-graphical recovery intact.

## 14. Decisions locked by this document

1. The first ISO uses conditional local `startx`; no display manager is required.
2. The graphical user session runs under `dbus-run-session` after PAM/elogind establish the authenticated login/session environment.
3. OpenRC user services are not a required first-ISO dependency for graphical-session startup.
4. X11/i3, Alacritty, PipeWire/WirePlumber and other intrinsically user-scoped graphical/session helpers are session-owned.
5. i3 configuration remains minimal, deterministic and configuration-driven, with `Mod4`, direct Alacritty access and standard recovery controls.
6. i3 IPC / `i3-msg` is the preferred workspace/window placement path; visual input is fallback rather than normal orchestration.
7. The first ISO does not require a compositor or VMware 3D acceleration.
8. The graphical session presents the MasterPortus workflow but does not own Master Portus/Codex lifecycle semantics.
9. GUI failure must preserve TTY, SSH, OpenRC and administrator recovery boundaries.
