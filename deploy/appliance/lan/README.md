# Appliance LAN profile (`ICN_APPLIANCE_LAN_PROFILE=1`)

Single-origin LAN serving for the **demo-profile** rehearsal appliance. Off by
default; requires `ICN_APPLIANCE_DEMO_PROFILE=1`. Never appropriate for
production or partner deployments — this is the same non-production rehearsal
posture, made reachable from an operator-controlled LAN through exactly one
browser origin.

## What it installs (at image build time)

- **nginx** inside the VM, terminating one origin (`ICN_APPLIANCE_LAN_ORIGIN`,
  e.g. `https://rehearsal.example.internal`) and forwarding:
  - `/v1/dev/demo/*` → `127.0.0.1:8091` — the demo-session endpoint **keeps its
    deliberate loopback-only bind**; the proxy is the only LAN path to it
  - `/v1/*` → `127.0.0.1:8080` — the gateway (JWT auth unchanged)
  - everything else → `/usr/share/icn/static/web` served as static files
    (the rehearsal landing page and the member shell), so every browser fetch
    is **same-origin**
- a rendered icnd drop-in (`30-lan-origin.conf`) whose `ICN_CORS_ORIGINS` is
  the demo profile's loopback set **plus exactly this one origin**
- a rendered demo-session drop-in setting
  `ICN_DEMO_SESSION_EXTRA_ORIGINS=<origin>` (exact-match, no wildcard)
- with an `https` origin: the operator-supplied certificate/key at
  `/etc/icn/tls/rehearsal.{crt,key}` (key mode 600) plus an HTTP→HTTPS
  redirect. Supplying a certificate is the operator's job (internal CA);
  the build never fabricates or disables TLS validation.
- `/etc/icn/lan-profile.env` recording the baked origin (non-secret marker;
  the typed appliance manifest schema is deliberately unchanged).

## Environment contract

| Variable | Required | Meaning |
|---|---|---|
| `ICN_APPLIANCE_LAN_PROFILE` | `1` to enable | opt-in; anything else = profile absent, image byte-identical to plain demo build |
| `ICN_APPLIANCE_LAN_ORIGIN` | yes | exact browser origin, `http(s)://host[:port]` — no path, no wildcard |
| `ICN_APPLIANCE_LAN_TLS_CERT` | if origin is https | PEM certificate (full chain) path on the build host |
| `ICN_APPLIANCE_LAN_TLS_KEY` | if origin is https | PEM private key path on the build host (never committed; never logged) |

## What it does NOT change

- No bind is widened: gateway was already `0.0.0.0:8080` in the demo profile,
  the session endpoint stays `127.0.0.1:8091`, the member-shell static server
  stays as-is (nginx serves the same tree directly).
- No auth or dev-gate semantics change; the session endpoint's server-side
  origin check and double dev-gate are unchanged — its allowlist simply gains
  the one configured origin.
- No public exposure: reachability beyond the VM's own LAN segment remains
  whatever the operator's network policy says. Do not port-forward or tunnel
  this origin to the public internet.
