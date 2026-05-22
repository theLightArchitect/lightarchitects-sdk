# Systemd Unit Files — Khadas Edge 2 Pro

Daemon sibling service units per **Canon XLIII — Sibling Substrate Partition** (2026-05-22).

## SSC Daemon Siblings

| Unit | Sibling | SSC Score | Clauses |
|------|---------|-----------|---------|
| `la-soul.service` | SOUL | 3 | [K] Neo4j + [O] AYIN + [P] fastembed |
| `la-eva.service` | EVA | 2 | [K] vaults + [O] hook pipeline |
| `la-ayin.service` | AYIN | 2 | [O] observability + [P] :3742 dashboard |
| `la-seraph.service` | SERAPH | 2 | [P] scan tools + [S] red-team principal |

## Install (Khadas, user scope)

```bash
mkdir -p ~/.config/systemd/user
cp deploy/systemd/la-*.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable la-ayin la-soul la-eva la-seraph
systemctl --user start la-ayin la-soul la-eva la-seraph
```

## Dependency order

```
la-ayin   (no deps — observability starts first)
la-soul   (wants la-ayin for trace reporting)
la-eva    (wants la-soul for vault integration)
la-seraph (independent, no wants)
```

## §N-1 Secret Migration (pending Canon N1B ratification)

`LoadCredential=` lines are present but commented out in each unit. Once `systemd-creds` +
TPM2 sealing is ratified (Canon N1B candidate #35 PROVISIONAL_QUEUED), uncomment the
`LoadCredential=` and `SetCredential=` lines and remove the corresponding `EnvironmentFile=`
references. See `LAEX-PHASE-7-QUEUE.md #35` for the migration plan.
