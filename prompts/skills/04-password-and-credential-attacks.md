# 04. Password & Credential Attacks

When: Hashes, login forms, auth tokens, memory dumps, or credential artifacts appear.

## Mental model
Credentials are heavily reused, weak by default, and leaked across memory, disk, and logs. Obtaining an existing secret key or valid identity is exponentially faster and stealthier than discovering a novel vulnerability.

## Attack arc
- Harvest Secrets:
  - *Process memory:* LSASS dumps (Mimikatz/procdump), web server memory, shell process memory.
  - *Static artifacts:* Configuration files (`.env`, `web.config`, `settings.py`, `wp-config.php`), git commit history (`trufflehog`, `gitleaks`), SSH keys, bash history.
  - *Network extraction:* Active Directory Kerberoasting (`$krb5tgs$`), AS-REP Roasting (`$krb5asrep$`).
- Offline Hash Identification & Cracking:
  - Identify hash algorithm (`hash-identifier`, `hashcat -m`).
  - High-performance offline cracking with `hashcat` or `john` using targeted dictionaries (rockyou) combined with rule-based mutations (`OneRuleToRuleTheyAll`, `dive.rule`, `best64`).
  - Mask attacks for known password policy patterns (e.g. `?u?l?l?l?l?d?d?d?s`).
- Online Password Spraying:
  - Execute slow, distributed spraying across target user lists to avoid lockout thresholds (e.g. 1 attempt per user every 30 minutes).
- Credential Reuse & Pass-the-Hash:
  - Authenticate using captured NTLM hashes directly (`pth-winexe`, `impacket-wmiexec`) without cracking.

## Key techniques & primitives
- Kerberoasting: Request service tickets (TGS) for accounts with Service Principal Names (SPN) and crack the RC4/AES-encrypted ticket offline.
- AS-REP Roasting: Request authentication tickets for users with `Do not require Kerberos preauthentication` and crack the returned encrypted timestamp.
- Rule-based Mutation: Transform base words with year appends, leetspeak, capitalization rules, and special character suffixes.

## Tells & signals
- Accounts with non-expiring passwords or disabled pre-authentication.
- Private keys (`id_rsa`, `.pfx`, `.pem`) stored in world-readable directories or source control.
- Absence of rate-limiting or account lockout on authentication endpoints.
