# 04. Password & Credential Attacks

When: Hashes, login forms, auth tokens, memory dumps, or credential artifacts appear.

## Mental model
Credentials are leaked across memory, disk, configuration files, and logs. High-leverage credential attacks operate on precision and signal: harvest artifacts first, test default credentials and pattern-based mutations next, and use cracking as a targeted lever rather than unguided guesswork. Balance credential attacks against application logic and code execution seams: if credentials do not yield immediate access, pivot to adjacent attack surfaces.

## Attack arc
- Harvest Secrets:
  - *Process memory:* LSASS dumps (Mimikatz/procdump), web server memory, shell process memory.
  - *Static artifacts:* Configuration files (`.env`, `web.config`, `settings.py`, `wp-config.php`), git commit history (`trufflehog`, `gitleaks`), SSH keys, bash history.
  - *Network extraction:* Active Directory Kerberoasting (`$krb5tgs$`), AS-REP Roasting (`$krb5asrep$`).
- Targeted Cracking & Verification:
  - Identify hash algorithm (`hash-identifier`, `hashcat -m`).
  - High-performance targeted cracking with `hashcat` or `john` using candidate lists and rule-based mutations (`OneRuleToRuleTheyAll`, `best64`).
  - Keep cracking loops tight and fast; if targeted masks fail to produce results quickly, re-examine the target for alternative auth flaws (e.g. key confusion, token forgery, logic bypasses).
- Credential Spraying & Reuse:
  - Test known service defaults and harvested accounts across exposed administrative interfaces.
  - Authenticate using captured NTLM hashes or tokens directly (`pth-winexe`, `impacket-wmiexec`) without cracking.

## Key techniques & primitives
- Kerberoasting: Request service tickets (TGS) for accounts with Service Principal Names (SPN) and crack the RC4/AES-encrypted ticket offline.
- AS-REP Roasting: Request authentication tickets for users with `Do not require Kerberos preauthentication` and crack the returned encrypted timestamp.
- Rule-based Mutation: Transform base words with year appends, leetspeak, capitalization rules, and special character suffixes.

## Tells & signals
- Accounts with non-expiring passwords or disabled pre-authentication.
- Private keys (`id_rsa`, `.pfx`, `.pem`) stored in world-readable directories or source control.
- Absence of rate-limiting or account lockout on authentication endpoints.
