# Deployment lanes (R18/R19)

Kaimeter runs the same single static binary in every lane — storage is
embedded SQLite everywhere, and customer data lives only where the customer
chooses. The only data Kaimeter always processes centrally is billing (US
soil, PCI-DSS processor). No telemetry of document or consignment data.

## Lane 1 — self-host (the default, live in one business day)

One file copy, no runtime install:

```sh
./kaimeter                      # serves http://127.0.0.1:8080
```

Or as a hardened service:

```sh
sudo cp deploy/kaimeter.service /etc/systemd/system/
sudo systemctl enable --now kaimeter
```

Or as a container (distroless/`scratch` — the image contains only the
binary):

```sh
docker build -t kaimeter .
docker run -v "$PWD/data:/data" -p 127.0.0.1:8080:8080 kaimeter
```

On-premises in the mainland works the same way — no ICP filing required
because nothing is hosted.

Configuration (env or `kaimeter.toml`): `KAIMETER_ADDR`
(default `127.0.0.1:8080`), `KAIMETER_DATA_DIR` (default `./data`),
`KAIMETER_LOCALES_DIR` (default `./locales`). Locale assets are embedded
in the binary; a locales directory is only needed to override them, and a
directory that exists but is incomplete refuses to start. Keep the address
loopback unless you front it with TLS — server-exposure mode
(LAN/internet) is a post-1.0.0 capability (R20).

## Lane 2 — Kaimeter-managed, customer's chosen region

Scripted cloud deployment from a locked, tested template. Supported
targets (R18): EU regions on AWS/Azure/GCP, Hong Kong on any global
region. The instance and its SQLite data stay inside the chosen region;
the storage backend and region are configuration, not code. ICR tenants
get per-EORI physical database segregation inside the instance
(`db/tenants/{eori}.db`, R25).

## Lane 3 — managed mainland China (ICP-bound, the exception)

Mainland hosting on Tencent/Alibaba/Huawei Cloud via a CN entity with ICP
filing. The filing takes ~4–8 weeks — this lane exists only on customer
request; self-hosting is always available and needs no filing (R19).

## Encryption at rest (R22)

Customer records are sealed with AES-256-GCM under a key derived from a
user-held passphrase (Argon2id, 64 MiB, 3 iterations) — a stolen device
yields ciphertext, not production data. Reference tables (public law
data) stay plaintext. See `src/vault.rs`.

## Data residency summary

| Lane             | Where data lives                     | Time to live |
| ---------------- | ------------------------------------ | ------------ |
| Self-host        | Customer hardware, any jurisdiction  | ≤ 1 day      |
| Managed EU/HK/US | Chosen cloud region, locked template | ≤ 1 day      |
| Managed mainland | CN cloud via CN entity, ICP on file  | 4–8 weeks    |

The only cross-border transfer Kaimeter creates is the EU filing the
regulation itself requires.
