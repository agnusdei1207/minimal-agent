# 01. Recon & Enumeration

**When:** Initial engagement phase or after discovering a new CIDR/hostname. If you cannot see the surface, you cannot exploit it.

## Mental model
Reconnaissance is **hierarchical surface reduction**: narrow from broad routing domains down to specific protocol parsers and versioned dependencies. **Sweep wide (fast SYN/UDP discovery) → probe narrow (precise service banners, headers, protocol handshakes, schema leaks).**

## Attack arc
- **Network discovery:** High-speed port sweeps (e.g. `nmap -sS -sV -T4`, `masscan`) on full range `1-65535` plus common UDP ports (DNS, SNMP, NTP).
- **Service fingerprinting:** Capture raw socket banners, TLS certificate Subject Alternative Names (SAN), HTTP Server/X-Powered-By headers.
- **DNS & Virtual Hosts:** Subdomain enumeration, zone transfers (AXFR), Host header fuzzing (`ffuf -H "Host: FUZZ.target"`), reverse IP lookups.
- **Web surface discovery:** Recursive directory/endpoint fuzzing, parameter discovery (`arjun`), API schema endpoints (`/swagger.json`, `/openapi.json`, `/graphql`, `/v1/api-docs`).
- **Cloud & Asset correlation:** ASN mapping, exposed S3/GCS bucket names, certificate transparency logs (crt.sh).
- **Structure data:** Build an in-memory or brief table: `Host | Port | Proto | Service | Version | Auth Req | Leads`.

## Key techniques & primitives
- **Virtual Host Routing:** Modern web apps serve different codebases based on the HTTP `Host` header; never test only the raw IP.
- **Parameter Mining:** Unlinked query/body parameters (`debug`, `test`, `admin`, `redirect`) frequently bypass frontend auth filters.
- **Hidden UDP Services:** TFTP, SNMP, IPMI, and SIP expose unauthenticated info and default credentials overlooked by TCP-only scans.

## Tells & signals
- 404 vs 403 on fuzzing = 403 indicates existing path requiring auth bypass or header mutation (`X-Forwarded-For: 127.0.0.1`).
- Differing TLS certificates on port 443 indicate co-hosted internal hostnames.
- Old major version banners with known public CVEs or default management interfaces (e.g. Tomcat Manager, phpMyAdmin, Jenkins, Kibana).
