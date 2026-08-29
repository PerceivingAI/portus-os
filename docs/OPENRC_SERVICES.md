# PortusOS OpenRC Service and Supervision Contract

**Last reviewed:** 2026-08-29T01:24:50Z
**Last updated:** 2026-08-29T01:24:50Z

**Status:** Authoritative for first-ISO lifecycle/supervision ownership; base Artix package/service names and default runlevels are locked, while installed dependencies/order and Portus-daemon supervision parameters still require target verification
**Target:** Artix/OpenRC first ISO
**Health authority:** `docs/HEALTH_RECOVERY.md`
**Runtime authority:** `docs/RUNTIME.md`
**Task authority:** `docs/TASKS.md`
**GUI/session authority:** `docs/GUI_SYSTEM.md`
**First-ISO authority:** `docs/ACCEPTANCE.md`

## 1. Purpose and authority

This document defines which layer owns lifecycle and supervision for machine services, user/session processes, Portus health/recovery, Portus tasks, and interactive agent sessions.

The ownership rule remains:

> **Every long-lived process has one primary lifecycle owner. OpenRC owns machine-scoped services; the authenticated user/session owns graphical and user-session processes; Portus health observes and coordinates recovery without becoming a competing init system; Portus tasks do not impersonate service supervision; Codex/tmux own interactive agent-session continuity rather than machine-service lifecycle.**

The engineering rule is equally conservative:

> **Preserve Artix/OpenRC's normal service model and runlevels; add only the dependencies and Portus services needed by the first ISO.**

## 2. OpenRC runlevel baseline

PortusOS does not invent a new first-ISO runlevel hierarchy.

The machine preserves the normal OpenRC startup model:

```text
sysinit
  -> boot
  -> default
```

Shutdown/reboot continue through OpenRC's normal shutdown semantics.

PortusOS may add service dependencies within that model, but does not create a parallel Portus boot graph.

Baseline runlevel policy:

- `sysinit`: leave distro/platform foundation to Artix/OpenRC; no Portus application daemon belongs here;
- `boot`: remains available for an installed service whose verified native Artix dependency contract requires early placement;
- `default`: the current first-ISO machine-readable selection for `dbus`, `elogind`, `NetworkManager`, `sshd`, `nftables`, `chrony`, `syslog-ng` and the intended Portus machine daemons.

`portusos-build/system/base-services.yaml` is authoritative for the currently locked base-service placement and selects `default` for all seven base services. Installed dependency/startup proof may justify a documented correction to that manifest; do not preserve an older generic assumption that D-Bus or elogind must be in `boot` when the selected Artix integration does not require it.

## 3. Machine-service package baseline

Current Artix L2 evidence locks the first-ISO base package/service mapping:

```text
system D-Bus       dbus + dbus-openrc                 service dbus
auth/session        elogind + elogind-openrc           service elogind
networking          networkmanager + networkmanager-openrc service NetworkManager
SSH                 openssh + openssh-openrc           service sshd
firewall            nftables + nftables-openrc         service nftables
time sync           chrony + chrony-openrc             service chrony
logging             syslog-ng + syslog-ng-openrc       service syslog-ng
```

`portusos-build/system/base-services.yaml` locks those identities and currently selects `default` for each. Installed script dependencies/startup behavior still require target execution. `docs/PACKAGE_POLICY.md` remains authoritative: do not replace a missing or changed Artix package with AUR automatically.

### 3.1 Networking

NetworkManager is the preferred first-ISO network manager because it provides normal Artix/OpenRC integration and a strong direct CLI through `nmcli`.

PortusOS does not wrap ordinary NetworkManager administration merely to provide another network CLI.

### 3.2 Firewall

Use nftables directly with a small PortusOS-owned ruleset.

Do not add firewalld or UFW merely as another policy layer unless a later requirement demonstrates a real need.

The firewall activation service must not depend on the GUI or Master Portus.

### 3.3 Time synchronization

Use one time-synchronization authority. Chrony is the selected first-ISO authority through the locked `chrony` + `chrony-openrc` package mapping.

VMware time integration must not periodically fight with the selected Linux time-sync service. Any VMware guest-time behavior must be configured around the Linux time authority rather than creating two competing clocks.

### 3.4 Logging

Use the normal lightweight Artix syslog path, with syslog-ng selected through the locked `syslog-ng` + `syslog-ng-openrc` package mapping.

PortusOS daemons integrate with native logging; PortusOS does not create a replacement machine logging subsystem.

## 4. Portus machine daemons

The following remain OpenRC-owned machine services:

- `portusd`;
- `portus-privd`;
- `portus-apid`.

They belong in the normal machine-service path, expected to be available independently of any graphical login or interactive Master session.

Their intended first-ISO runlevel is `default`, subject to final dependency verification.

Dependencies must reflect real prerequisites. Examples:

- a Portus daemon that requires system D-Bus declares that dependency rather than relying on arbitrary start order;
- a daemon must not depend on X11/i3 merely because some consumers are graphical;
- SSH/network/logging recovery must not depend on a Portus daemon.

## 5. OpenRC supervision mechanism

For Portus-owned machine daemons that are classified as restart-safe, use OpenRC-native `supervise-daemon` rather than introducing runit, s6, systemd, or another general-purpose supervisor.

The daemon must remain in the foreground for supervision.

Exact `respawn_delay`, `respawn_max`, `respawn_period`, health-check cadence and unhealthy behavior are **per-service engineering parameters** owned jointly with `docs/HEALTH_RECOVERY.md`; they are not guessed globally before the real daemon behavior is measured.

Rules:

- use OpenRC's normal start/stop/status functions where required by `supervise-daemon`;
- no infinite crash-loop policy by default;
- a service classified as unsafe to restart automatically stays manual/recovery-driven;
- Portus health observes restart exhaustion but does not secretly start a second respawner.

## 6. User/session-owned processes

The graphical/user-session class includes:

- X11 session startup;
- i3;
- Alacritty presentation terminals;
- user D-Bus;
- PipeWire and WirePlumber when audio is included;
- graphical notification agents;
- graphical polkit/keyring/auth agents;
- other helpers that exist only for the logged-in user session.

They may depend on machine services such as D-Bus or elogind, but that does not transfer lifecycle ownership to OpenRC.

### 6.1 First-ISO user-session mechanism

OpenRC supports user services, but PortusOS does **not** make OpenRC user-service orchestration a first-ISO dependency.

The first-ISO baseline is:

```text
PAM/elogind authenticated login
  -> XDG_RUNTIME_DIR/session context
  -> conditional startx
  -> dbus-run-session
  -> Portus graphical-session launcher
  -> i3 + selected user-session helpers
```

PipeWire/WirePlumber and similar user helpers start from the authenticated graphical-session path or another verified session-native mechanism, not as machine daemons.

This can be revisited later if direct Artix testing shows OpenRC user services provide a materially cleaner supported path.

## 7. Master Portus, Codex and tmux boundary

Master Portus/Codex is not a generic boot daemon.

The local path remains:

```text
authenticated Master user login
  -> user/session environment
  -> X11/i3 for local graphical presentation
  -> Alacritty / MasterPortus tmux attachment
  -> portus-bootstrap
  -> portus-master
  -> Codex
```

For the first ISO:

- OpenRC must not start an interactive Codex TUI merely because the machine booted;
- tmux preserves interactive attachment/presentation continuity, not machine-service authority;
- `portus-master` does not become a replacement init system;
- SSH login remains independent and must not automatically create the graphical session;
- headless/managed Codex execution follows the task/session contract rather than being disguised as an OpenRC service.

## 8. Portus health/recovery boundary

Portus health may:

- inspect OpenRC/native service state;
- correlate service failure with provider/index/task impact;
- record bounded health/recovery history;
- request permitted restart of a restart-safe OpenRC service;
- detect restart-budget exhaustion;
- report administrator/manual recovery needs.

Portus health must not:

- maintain a contradictory private service-state machine;
- respawn an OpenRC-owned service behind OpenRC's back;
- change service runlevels/dependencies implicitly;
- classify an unsafe service as restart-safe merely to recover automatically;
- restart session-owned processes as machine daemons.

OpenRC/native state remains authoritative for machine-service lifecycle.

## 9. Portus task boundary

Portus tasks may reference or depend on machine services but do not supervise them.

Task-owned ordinary processes follow the managed-execution/task contract rather than being turned into system services merely to obtain restart behavior.

## 10. PortusBrowser boundary

P15 resolves the first-ISO Broker lifecycle: **provider-owned / native-host-on-demand**. The PortusBrowser native-messaging host starts the Broker when it is missing, matching the provider's current architecture.

PortusOS must therefore **not** add an OpenRC Broker service or any second supervisor. Linux integration still has to verify that the installed package/config preserves this single lifecycle owner and that Broker startup/recovery works reliably through the native-host path.

The install contract supplies package-owned templates for `portusd`, `portus-privd`, and `portus-apid` under `runtime/install/openrc/`, staged only to `/usr/share/portus/openrc/templates/`. They are deliberately not `/etc/init.d` scripts: each unresolved template carries the internal `P16-LINUX-RESOLUTION-REQUIRED` fail-closed marker, and host-safe validation rejects a template that prematurely freezes `depend()`, supervisor/respawn behavior, or an unresolved service identity. The verified Artix installation path renders the real init scripts only after the remaining service facts are established.

## 11. Exact-service-name policy

Do not freeze service names from generic Linux assumptions.

On the reference Artix environment:

1. install the selected official package plus its OpenRC integration package where applicable;
2. inspect the actual `/etc/init.d` scripts and package files;
3. inspect dependency declarations;
4. exercise `rc-service`, `rc-status` and `rc-update`;
5. freeze the verified service identity/runlevel/dependency into the machine-readable service manifest and this document.

This is verification, not an owner-level architecture decision.

## 12. Remaining OpenRC verification

Still to verify/freeze:

- installed behavior of the locked D-Bus, elogind, NetworkManager, OpenSSH, nftables, chrony and syslog-ng service identities and `default` placement;
- exact dependency declarations/startup/shutdown ordering from the real installed scripts;
- exact `portusd`, `portus-privd`, `portus-apid` OpenRC scripts;
- per-daemon restart-safe classification and `supervise-daemon` parameters/health checks;
- exact PAM/elogind package/config mapping used by the graphical session;
- verify the installed PortusBrowser package preserves the P15 provider-owned/native-host-on-demand Broker lifecycle and contains no competing OpenRC owner;
- service users/groups and any additional first-ISO daemon mappings.

These verification items may refine names and dependencies but do not reopen the lifecycle/runlevel strategy without a concrete incompatibility.

## 13. First-ISO acceptance implications

Validation must prove at minimum that:

- the installed system follows normal OpenRC `sysinit -> boot -> default` startup;
- D-Bus/elogind prerequisites support the authenticated local session;
- NetworkManager, SSH, firewall, time sync and logging reach the selected OpenRC-owned state;
- `portusd`, `portus-privd` and `portus-apid` are OpenRC-owned and available without a GUI login;
- restart-safe Portus daemons use OpenRC-native supervision with bounded failure behavior;
- stopping/failing Master Portus or X11/i3 does not collapse networking/SSH/logging/core recovery;
- graphical helpers remain session-owned;
- Portus health reports OpenRC failures without inventing contradictory lifecycle state.

## 14. Decisions locked by this document

1. Every long-lived process has one primary lifecycle owner.
2. OpenRC owns machine-scoped services; authenticated sessions own graphical/user-session processes.
3. PortusOS preserves the normal OpenRC `sysinit -> boot -> default` startup model and creates no parallel Portus runlevel hierarchy.
4. System D-Bus and elogind are intended early machine/session prerequisites; ordinary networking/SSH/firewall/time/logging and Portus daemons use the normal machine-service path, generally `default`, subject to native Artix script verification.
5. NetworkManager is the preferred first-ISO network manager; native `nmcli` remains directly usable.
6. nftables is the preferred firewall mechanism without an added firewalld/UFW layer.
7. Chrony is the selected first-ISO time-sync authority through `chrony` + `chrony-openrc`.
8. syslog-ng is the selected first-ISO machine-logging authority through `syslog-ng` + `syslog-ng-openrc`.
9. `portusd`, `portus-privd` and `portus-apid` are OpenRC-owned machine services, intended for `default` after dependency verification.
10. Restart-safe Portus daemons use OpenRC `supervise-daemon`; no second general-purpose supervisor is added.
11. OpenRC user services are not a required first-ISO graphical-session dependency.
12. PAM/elogind + `dbus-run-session` + the authenticated session own graphical/user helpers.
13. Master Portus/Codex is not a generic boot daemon; tmux owns attachment continuity only.
14. SSH/recovery remain independent of Master Portus and the GUI.
15. The PortusBrowser Broker retains the P15 provider-owned/native-host-on-demand lifecycle with no competing OpenRC owner after installation.
16. The first-ISO base service names/runlevels are frozen in the machine-readable manifest; installed dependency/order behavior and Portus-owned service scripts remain real-target verification.
