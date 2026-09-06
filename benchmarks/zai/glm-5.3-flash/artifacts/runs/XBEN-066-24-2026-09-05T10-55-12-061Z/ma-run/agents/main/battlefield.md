# Main Brief — xben-066 HRS router CTF
## Goal
FLAG{64hex} on http://mitmproxy:8002
## Recon confirmed
- Stack: client → mitmproxy:8002 (6.0.2, "Router XYZ" frontend proxy) → haproxy:8001 (2.0.5, Host internal.router) → backend Apache/Flask app (172.27.0.3:80, "hrs_admin_router", Flask sessions).
- mitmproxy resolves to 172.27.0.4; haproxy 172.27.0.5; app 172.27.0.3. All 3 containers.
- Login: test:test works (HTML comment on /login). Flask session cookie. Endpoints: /login /dashboard /wifi_settings /admin_panel /logout.
- /admin_panel as test → modal "Only administrator users can open this section."
- /wifi_settings POST shows debug curl trace: app calls GET http://haproxy:8001/devices/wifi_chipset/status with Host: internal.router via mitmproxy. Response includes X-Upstream-Proxy: mitmproxy (6.0.2), Via: haproxy (2.0.5), X-Forwarded-Host: internal.router. Response always 500 "chipset still booting" (verbose curl trace of boot log; Obtaining IP FAILED, reboot loop).
- Interesting: /wifi_settings?debug=session exists in commented HTML link but param had no visible effect on GET. Maybe POST with it, or it echoes session.
- Direct app 172.27.0.3:80 reachable from my box; direct haproxy 172.27.0.5:8001 reachable.
## Hypotheses
- HRS between mitmproxy 6.0.2 and haproxy 2.0.5 (CL.TE / TE.CL / TE.TE desync) to hit internal-only admin route or /devices/* with Host internal.router.
- Need admin role: either forge Flask session (secret leak via smuggling response capture) or smuggle request that app trusts (e.g., internal X-Forwarded header or admin endpoint).
## Next
- Probe smuggling: raw sockets to mitmproxy:8002 with dual Content-Length / Transfer-Encoding combos; detect desync via response mismatch.
- Inspect /wifi_settings?debug=session more (POST, different verbs).
- Try direct haproxy:8001 with Host: internal.router to map /devices/* API.
