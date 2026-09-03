# 06. Windows & Active Directory

When: Operating in a Windows domain environment or holding a low-privileged Windows user/service shell.

## Mental model
Active Directory is a graph of identity relationships and delegation permissions. Domain compromise is walking the shortest path from an initial low-privileged account to Domain Admin (DA) or Enterprise Admin by exploiting misconfigured trusts, ticket mechanics, and certificate templates.

## Attack arc
- Domain Graph Enumeration:
  - Collect domain objects, trusts, session details, and ACLs via BloodHound / SharpHound or LDAP queries.
  - Enumerate users, high-value groups, computers, Domain Controllers (DCs), and Certificate Authorities (ADCS).
- Kerberos & Ticket Abuse:
  - *Kerberoasting:* Request RC4/AES Service Tickets (TGS) for accounts with SPNs and crack offline.
  - *AS-REP Roasting:* Harvest encrypted pre-auth timestamps for accounts without pre-auth required.
  - *Pass-the-Hash (PtH) / Overpass-the-Hash:* Authenticate to SMB/WinRM using NTLM hash or convert to a Kerberos TGT.
  - *Delegation Abuse:* Unconstrained Delegation (capture TGT in memory), Constrained Delegation (`s4u2self` / `s4u2proxy`), Resource-Based Constrained Delegation (RBCD).
- ADCS (Active Directory Certificate Services) Exploitation:
  - Audit certificate templates for ESC1 (Client Auth + `CT_FLAG_ENROLLEE_SUPPLIES_SUBJECT`), ESC2/ESC3 (Any Purpose / Enrollment Agent), ESC4 (Vulnerable ACLs), ESC8 (NTLM Relay to HTTP enrollment).
  - Request certificates with forged SAN (Subject Alternative Name) for Domain Admin and request TGT with PKINIT (`certipy`, `gettgtpkinit`).
- Local Windows Privilege Escalation:
  - Unquoted service paths, writable service executables, DLL hijacking, AlwaysInstallElevated.
  - Token impersonation: SeImpersonatePrivilege / SeAssignPrimaryTokenPrivilege via PrintSpoofer, GodPotato, or SweetPotato.
- Domain Looting (Persistence/Domination):
  - DCSync attack via `secretsdump.py` abusing `DS-Replication-Get-Changes` rights to extract all NTLM hashes including `krbtgt`.

## Key techniques & primitives
- ADCS ESC1: Misconfigured template allowing any authenticated user to supply a custom UPN in the SAN, obtaining a certificate for DA and converting it to a Kerberos TGT.
- RBCD (Resource-Based Constrained Delegation): Write `msDS-AllowedToActOnBehalfOfOtherIdentity` attribute on a computer object to impersonate administrative users to that computer.
- Named Pipe Impersonation: Trigger SYSTEM RPC connection to a controlled named pipe and call `ImpersonateNamedPipeClient()` to elevate privileges.

## Tells & signals
- Service accounts running with administrative privileges or SPNs assigned to standard user accounts.
- Active Directory Certificate Authorities deployed with default vulnerable templates (`SubCA`, `User`, or custom enrollment templates).
- `whoami /priv` displaying `SeImpersonatePrivilege: Enabled`.
