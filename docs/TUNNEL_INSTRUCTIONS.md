# Secure MCP Tunnel Setup

**Last reviewed:** 2026-08-29T01:24:50Z
**Last updated:** 2026-08-29T01:24:50Z

## PortusOS first-boot role

`portus-mcp` and OpenAI `tunnel-client` are canonical first-ISO components. They are **present on the live ISO and installed system**, but their remote-tunnel setup is optional and user/session-owned rather than an OpenRC machine service.

The intended first interactive boot order is:

```text
boot PortusOS
  -> log in as Master Portus
  -> reach X11/i3 + Alacritty + MasterPortus tmux
  -> authenticate/start Codex
  -> optionally configure Portus MCP + Secure MCP Tunnel
  -> remote ChatGPT/Codex can connect to this machine through Portus MCP
```

Skipping tunnel setup must leave local Codex and the rest of PortusOS fully usable. The ISO must not contain a pre-provisioned tunnel ID, runtime API key, admin key, or other reusable owner credential.

PortusOS bundles:

```text
/opt/portus/portus-mcp
/usr/local/bin/tunnel-client
```

`portus-mcp` requires Node.js 20.9+ and npm; those are first-ISO packages because Portus MCP is bundled, not because Codex needs npm. Its canonical local MCP endpoint is `http://127.0.0.1:8789/mcp`.



Current first-ISO frozen inputs are tunnel-client `0.0.13` and Portus MCP source revision `a963c56a72ca106cbac3b848256bb393fb9dc2da`; the exact tunnel-client Linux x86_64 digest remains machine-owned by the component contract rather than duplicated here.

The pinned tunnel-client release is recorded in `portusos-build/components/tunnel-client.yaml`. The binary also carries OpenAI's Codex tunnel plugin bundle; after Codex login, the optional setup path may run:

```bash
tunnel-client codex plugin install
tunnel-client codex status
tunnel-client codex diagnose --json
```

For the Portus MCP tunnel, use profile name `portus-local` and local target `http://127.0.0.1:8789/mcp`. Runtime setup should require only the user's tunnel ID and runtime API key; setup must never silently generate or embed credentials in the ISO.

---

On PortusOS, `tunnel-client` is already installed at `/usr/local/bin/tunnel-client` and Portus MCP is already staged at `/opt/portus/portus-mcp`. The `~/.local/bin` and developer-checkout paths later in this document apply only to non-PortusOS development hosts.

## PortusOS quick setup

After `portus-bootstrap` has prepared the Master workspace and Codex is authenticated, the normal first-ISO path is one command:

```bash
portus-tunnel-setup
```

On the first run it:

1. prompts without echo for the OpenAI tunnel runtime API key;
2. prompts for the tunnel ID;
3. creates the fixed `portus-local` profile targeting `http://127.0.0.1:8789/mcp`;
4. installs the bundled tunnel-client Codex plugin;
5. registers `/workspace/$USER/master` as the default Portus MCP project;
6. starts Portus MCP, waits for readiness, then starts `tunnel-client`;
7. keeps both processes observable in the terminal until Ctrl+C.

On later runs, an existing `portus-local` profile is reused and only a runtime API key is required unless `CONTROL_PLANE_API_KEY` is already set in that terminal.

To run Portus MCP locally without exposing a tunnel:

```bash
portus-mcp-local
```

The first-ISO helper deliberately does **not** persist the tunnel API key. While the combined process is running, the key exists in the Master-owned process environment and is therefore not claimed to be protected from Master Portus. Do not put it in shell startup files or the ISO.

## Prerequisites

You need:

- An OpenAI `tunnel_id` from [Platform Tunnel Settings](https://platform.openai.com/settings/organization/tunnels)
- A runtime OpenAI API key with Tunnels **Read** + **Use** permissions
- A local MCP server available over stdio or HTTP

Never paste the API key into chat or commit it to a file or repository.

## Set credentials

Set the credentials in the terminal where you will configure and run the client:

```bash
read -r -p "Tunnel ID: " CONTROL_PLANE_TUNNEL_ID
read -r -s -p "OpenAI runtime API key: " CONTROL_PLANE_API_KEY
echo
export CONTROL_PLANE_TUNNEL_ID CONTROL_PLANE_API_KEY
```

The API key is hidden while typing. The environment variables last only for the current terminal session.

## Configure a local stdio MCP server

Replace `YOUR_MCP_SERVER_COMMAND` with the command that starts your MCP server:

```bash
tunnel-client init \
  --sample sample_mcp_stdio_local \
  --profile local-stdio \
  --tunnel-id "$CONTROL_PLANE_TUNNEL_ID" \
  --mcp-command "YOUR_MCP_SERVER_COMMAND"
```

Examples:

```bash
--mcp-command "python /home/user/path/to/server.py"
```

```bash
--mcp-command "npx -y @modelcontextprotocol/server-filesystem /home/user/Documents"
```

The generated profile stores `env:CONTROL_PLANE_API_KEY`, a reference to the environment variable. It does not store the API key itself.

## Configure an HTTP MCP server

Replace the URL with the address of your private MCP server:

```bash
tunnel-client init \
  --sample sample_mcp_remote_no_auth \
  --profile remote-http \
  --tunnel-id "$CONTROL_PLANE_TUNNEL_ID" \
  --mcp-server-url "http://127.0.0.1:3000/mcp"
```

For an HTTPS server, use its HTTPS URL:

```bash
--mcp-server-url "https://mcp.internal.example.com/mcp"
```

## Validate the profile

For the stdio profile:

```bash
tunnel-client doctor --profile local-stdio --explain
```

For the HTTP profile:

```bash
tunnel-client doctor --profile remote-http --explain
```

The doctor command checks the profile, credentials, control-plane reachability, and local MCP configuration.

## Start the tunnel

Start the profile you configured:

```bash
tunnel-client run --profile local-stdio
```

Or:

```bash
tunnel-client run --profile remote-http
```

Keep this terminal running. OpenAI products depend on the client remaining connected.

## Check local status

The local operator UI is normally available at:

```text
http://127.0.0.1:8080/ui
```

Health checks:

```bash
curl -fsS http://127.0.0.1:8080/healthz
curl -fsS http://127.0.0.1:8080/readyz
```

## Connect from ChatGPT

1. Open [ChatGPT Plugins](https://chatgpt.com/plugins).
2. Create a developer-mode app.
3. Select **Tunnel** as the connection type.
4. Select the configured tunnel, or enter its valid `tunnel_id`.
5. Test the MCP tools while `tunnel-client run` remains active.

If the tunnel is not listed, verify that it is associated with the target ChatGPT workspace and that your account has Tunnels **Read** + **Use** access.

## New terminal sessions

The environment variables are not persistent. Before running the client in a new terminal, set them again:

```bash
export CONTROL_PLANE_API_KEY="your-key"
```

The configured profile continues to use `env:CONTROL_PLANE_API_KEY`.

Avoid putting the raw key in `.bashrc`, `.profile`, or the tunnel profile. For persistent unattended operation, use a dedicated service manager secret mechanism instead.

## Useful commands

```bash
tunnel-client --version
tunnel-client help quickstart
tunnel-client doctor --profile local-stdio --explain
tunnel-client run --profile local-stdio
```

Configuration profiles are stored under:

```text
~/.config/tunnel-client/
```

## Troubleshooting

### `Cannot find package 'dotenv'`

The Portus dependencies are missing or incomplete. From the Portus repository, run:

```bash
cd /path/to/portus-mcp
npm install
```

Then retry:

```bash
npm run start:tunnel
```

### Invalid local policy file

An error such as:

```text
main_agent.permissions.projectScreenshot: Required
limits.screenshot: Required
```

means `portus-mcp.policy.local.json` is older than the current policy schema. The simplest fix, if a custom local policy is not required, is to set this in the Portus `.env` file:

```env
PORTUS_MCP_POLICY_PATH=./portus-mcp.policy.json
```

If the local policy is intentional, copy the missing `projectScreenshot` and complete `limits.screenshot` settings from `portus-mcp.policy.json` into `portus-mcp.policy.local.json`. Policies are complete replacements; they are not merged.

### `profile_load FAIL ... portus-local.yaml: no such file`

Create the profile explicitly:

```bash
tunnel-client init \
  --profile-dir "$HOME/.config/tunnel-client" \
  --profile portus-local \
  --tunnel-id "tunnel_..." \
  --mcp-server-url "http://127.0.0.1:8789/mcp"
```

Replace `tunnel_...` with the real tunnel ID. The profile should then exist at:

```text
~/.config/tunnel-client/portus-local.yaml
```

### `control_plane_api_key ... environment variable ... is not set`

The profile references `env:CONTROL_PLANE_API_KEY`, but that variable is absent from the terminal running `doctor` or `npm run start:tunnel`. Export it in the same terminal:

```bash
export CONTROL_PLANE_API_KEY="your-runtime-api-key"
```

Do not use an admin key for the long-running client.

For a safer local setup, store only the raw key in a protected file:

```bash
nano "$HOME/.config/tunnel-client/api-key"
chmod 600 "$HOME/.config/tunnel-client/api-key"
```

Paste only the key, with no quotes. Then set the profile's API key reference to:

```yaml
api_key: "file:/home/user/.config/tunnel-client/api-key"
```

Do not commit the key file or paste the key into chat.

### `[tunnel] Exited with code 1`

This means the tunnel client stopped; it is not proof that the tunnel is working. Run the client directly to expose its diagnostic:

```bash
tunnel-client run \
  --profile-dir "$HOME/.config/tunnel-client" \
  --profile portus-local \
  --log.level debug
```

A working client remains running and does not immediately return to the shell.

### `mcp_server_reachable ... connection refused`

The tunnel profile is valid, but Portus MCP is not listening at the configured URL. Start the integrated Portus launcher from the repository:

```bash
cd /path/to/portus-mcp
npm run start:tunnel
```

This starts Portus MCP, waits for `http://127.0.0.1:8789/mcp` to become ready, and then starts `tunnel-client --profile portus-local`. Keep the command running.

Validate from a second terminal:

```bash
tunnel-client doctor \
  --profile-dir "$HOME/.config/tunnel-client" \
  --profile portus-local \
  --explain
```

The required checks should pass:

```text
profile_load
tunnel_id
control_plane_api_key
mcp_target
mcp_server_reachable
```

The Node `DEP0205` `module.register()` deprecation warning is unrelated to tunnel connectivity.

### ChatGPT cannot see the tunnel

Verify that:

1. The tunnel is associated with the target ChatGPT workspace.
2. The account connecting the app has Tunnels **Read** + **Use**.
3. `npm run start:tunnel` is still running.
4. The tunnel ID belongs to the intended Platform organization.

ChatGPT workspace association is separate from Platform organization tunnel permissions.
