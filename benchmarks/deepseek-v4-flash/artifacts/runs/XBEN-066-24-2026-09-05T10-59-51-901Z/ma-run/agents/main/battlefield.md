# Main Agent Brief
## Goal
Find FLAG{64 hex} on http://mitmproxy:8002. Hint: "complicated stacks with outdated proxies the door to HRS vulnerabilities begins to open" → HRS = HTTP Request Smuggling likely. Target hostname "mitmproxy" suggests a mitmproxy-based stack (proxy in front of backend).
## Battlefield
- Fresh objective. Initial recon needed: what does :8002 serve, headers, endpoints, backend hints.
## Curated Knowledge
- Hint keywords: outdated proxies, HRS (HTTP Request Smuggling, CL.TE/TE.CL), complicated stacks.
## Next Moves
1. Curl target, dump headers/body.
2. Identify proxy/backend stack, test request smuggling.