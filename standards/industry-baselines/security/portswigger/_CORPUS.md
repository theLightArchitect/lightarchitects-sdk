---
name: portswigger-corpus-index
description: PortSwigger (Burp Suite) + Wiley industry baselines for web application penetration testing — Web Application Hacker's Handbook 2nd ed
type: reference
authority_rating: HIGH
updated: 2026-06-02
---

# PortSwigger / Wiley Industry Baselines — Corpus Index

Canonical pentest methodology references. Complements `security/ptes/` (methodology) with practical attack catalogue.

## GCS Location

```
gs://la-platform-helix/user/standards/industry-baselines/security/agent-and-pentest/
└── The Web Application Hackers Handbook 2nd Edition.pdf  (13.5 MB, 853 pp)
```

Mirrors this helix path: `$HELIX/user/standards/industry-baselines/security/portswigger/`

## Vertex AI Search

- **Project**: `webshell-497114`
- **Data store**: `la-security-baselines`
- **Engine**: `la-search`
- **Status**: pending re-import (PDF saved 2026-06-02; needs .txt copy + reindex per [[2026-06-02-discovery-engine-mime-type]] pattern)

## Why a Separate Subfolder

- **PortSwigger** (Dafydd Stuttard) created Burp Suite — the de facto integrated testing tool
- **Wiley** is the publisher of foundational security texts (WAHH 2nd ed, Hacking Exposed series, etc.)
- These references are **complementary** to PTES (methodology) and OWASP (vulnerability classes)
- PTES says *what to test*; OWASP says *what vulnerabilities exist*; WAHH says *how to find and exploit them in practice*

## Documents in this Folder

| File | Authority | Relevance |
|------|-----------|-----------|
| `wahh-2nd-edition-2011-2026-06-02.md` | HIGH (Wiley + Burp co-author) | 853pp / 21-chapter web pentest methodology |

## Citation Anchors (for reuse in helix entries)

- 13-step WAHH methodology (Ch. 21): 1.Map content → 2.Analyze → 3.Client-side → 4.Auth → 5.Session → 6.Access → 7.Input fuzz → 8.Specific input → 9.Logic → 10.Shared hosting → 11.App server → 12.Misc → 13.Info leak
- Authentication attack surface (Ch. 6): 14 sub-techniques
- Session management weaknesses (Ch. 7): 10 token-handling vulns
- Injection coverage (Ch. 9-10): SQLi, NoSQL, XPath, LDAP, OS Cmd, Path Traversal, XXE, SOAP, SMTP
- Application logic flaws (Ch. 11): 12 real-world example patterns
- Source-code review cheat sheets (Ch. 19): Java / .NET / PHP / Perl / JS dangerous-API lists
- Burp Suite integrated testing pattern (Ch. 20)
- Nikto/Wikto for default content discovery
- Hydra for password guessing automation
