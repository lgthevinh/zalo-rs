# Errors, retries and backoff

Every Zalo `error_code` the previous client encountered or handled, what it means, and how a client should react, plus HTTP-level and transport failures and the retry/backoff rules. The old troubleshooting guide was deleted; this table was rebuilt from code, tests and existing specs only.

Related docs: [../auth/session.md](../auth/session.md), [../auth/service-discovery.md](../auth/service-discovery.md) (host failover), [../security/payload-encryption.md](../security/payload-encryption.md), [../realtime/websocket.md](../realtime/websocket.md), [../call/](../call/).

---

## 1. Response envelope and where the code lives

- `[O]` REST responses are JSON `{ "error_code": <int>, "error_message": <string>, "data": … }`; `0` = success.
- `[I]` When the request was param-encrypted, `data` is a base64 AES blob; once decrypted it is **itself a full envelope with its own `error_code`**. A client must check the outer code **and** the decrypted inner code. An outer `0` with an inner non-zero is a failure (live example: outer `0`, inner `18022` `[O]`).
- `[I]` A non-JSON body is a bare base64 encrypted blob; hand it to decryption rather than failing.
- `[O]` HTTP 200 with an empty body on long-poll hosts = keepalive timeout, treat as `{error_code: 0, data: null}`.
- `[I]` A missing `error_code` counts as `0` for ordinary calls, but as "no verdict" for the session-validity check (§4).

---

## 2. `error_code` table

| Code | Message / meaning | Where seen | Client action | Tag |
|---|---|---|---|---|
| `0` | success | everywhere | — | `[O]` |
| `600` | `"zpw_sek not found or invalid"` — session cookie missing/invalid/expired | any authenticated endpoint (e.g. `get-last-msgs` without a session) | **Session expired**: stop, mark stored session expired (keep the file for diagnostics until a new login replaces it), require re-login (QR). Do not fail over to another host. | `[O]` |
| `-102` | session no longer usable | treated as auth failure by the previous client | Same as `600` | `[I]` |
| `18060` | `"Invalid encryption protocol"` — plain (unencrypted) params where ParamCipher v2 is required | `loginByPasswv2` | Protocol bug: send encrypted params. **Not** a wrong-password signal. Surface as internal error. | `[O]` |
| `18022` | `"Invalid Signature"` — `signkey` missing/wrong | `getServerInfo` without signkey; inside the decrypted login reply | Protocol bug: fix signing ([../security/signkey.md](../security/signkey.md)). Do not retry unchanged. | `[O]` |
| `18012` | QR scanned on phone, waiting for user to tap confirm | QR confirm poll (`pc-status`) | Keep polling (~800 ms) until `0` or timeout | `[I]` |
| `111` | `"invalid param"` — a required field is missing | voice-call signalling probes; generic | Surface; do not retry unchanged | `[I]` |
| `114` | bad ID form/size, or action not allowed for this target (e.g. calling a non-friend; `getminiprofiles` with raw uid lists) | voice call, profiles; used as the failure that triggers send fallbacks | Surface; for link/quote send, fall back to a plain text send with the **same** `clientId` | `[I]` |
| `-101` | value does not resolve to anything live (e.g. bogus session/call id) | voice-call probes | Surface | `[I]` |
| `216` | phone number not registered on Zalo, or user blocks search by phone | find user by phone | Surface to the user ("not found") | `[I]` |
| other non-zero | unknown business error | anywhere | Surface with code + message; **never** treat as session expiry; never fail over hosts | `[I]` |
| `401`–`416`, `12006`–`12447` | **not error codes**: VoIP signalling command / signal IDs | voice call | See [../call/](../call/) | `[I]` |

Codes the task brief mentioned but for which **no evidence exists** in code or specs: `102` (positive), `19000`, `429` (as `error_code` or HTTP) — `[U]`. Do not invent handling; log and surface.

`[I]` Non-zero `error_code` on QR "waiting for scan" polling is tolerated up to the server-provided `qr_retry` count before giving up.

---

## 3. HTTP and transport failures

| Condition | Meaning | Action | Tag |
|---|---|---|---|
| HTTP 2xx, JSON | normal | check `error_code` (§1) | `[O]` |
| HTTP 200, empty body | long-poll window expired | treat as success, no data; poll again | `[O]` |
| HTTP 408 on QR wait-scan / confirm | long poll expired, nothing scanned/confirmed yet | retry after ~800 ms until the QR timeout (≥ 30 s scan, ≥ 60 s confirm) | `[I]` |
| HTTP 404 | wrong host flavour (e.g. `*.zaloapp.com` vs `*.chat.zalo.me` → nginx 404) | **fail over** to the next candidate host for that service | `[O]` (404s from wrong flavour), `[I]` failover |
| connection refused, timeout, DNS error | transport | fail over to the next candidate; if all fail → "unreachable" | `[I]` |
| other non-2xx (e.g. 500) | server/transport error | error, no failover; classify as "unreachable" | `[I]` |
| WebSocket handshake `501 Not Implemented` | empty request path | add `/` before `?` ([../realtime/websocket.md](../realtime/websocket.md)) | `[I]` |
| WebSocket close `3002 NOT_FOUND_WORKER` | shard doesn't host this session/queue | reconnect to the next `zpw_ws` entry; never pull unlisted queues | `[I]` |
| WebSocket AUTHEN `error_code ≠ 0` | auth rejected on socket | close, reconnect with backoff; `[U]` whether a specific code implies session expiry | `[I]` |
| Response decryption fails | wrong key or not encrypted | keep raw value; treat as error for the call | `[I]` |

Host failover rules `[I]`: candidates per service in priority order (verified constant, then service-map entries, then previously saved host); remember the winning host in memory for the rest of the session. **Business/auth errors (any `error_code`) never fail over.**

---

## 4. Session-validity classification — `[I]`

```
classify(result):
    error_code == 0            -> Valid
    error_code in {600, -102}  -> Expired      (re-login required)
    any other error_code       -> Unreachable  (no verdict)
    transport/HTTP/parse error -> Unreachable
    no error_code in body      -> Unreachable  (for the explicit validity check)
```

- The explicit check calls `GET {profile}/api/social/profile/me-v2` with encrypted `{avatar_size: 160, imei}` and `os=8` ([../auth/session.md](../auth/session.md)).
- **Unreachable never destroys the stored session**; report "cannot reach Zalo" and retry later. Only Expired triggers logout + re-login, and even then keep the old session file (marked expired) until a new login overwrites it.
- A missing local credential (no login / no `zpw_sek`) is Expired.

---

## 5. Retry and backoff schedule (previous client)

All `[I]`; `[U]` what the official client does.

| Loop | Normal cadence | On error | Logged out |
|---|---|---|---|
| WebSocket reconnect | — | 2 s, doubling, cap 30 s; rotate `zpw_ws` index each attempt (reset the backoff after a successful AUTHEN — the previous client forgot to) | socket dropped; check every 3 s |
| WebSocket supervisor | read with 1.5 s timeout | channel closed → wait 2 s, respawn | — |
| WebSocket ping | every 15 s after auth | — | — |
| `lp_chat` message poll | tick every 1.2 s; skip if in flight; up to 3 drain rounds while `more == 1` or reset | Expired → re-login; otherwise report unreachable and retry next tick | skipped |
| `lp_chat` request timeout | 12 s client timeout → treat as empty | — | — |
| `lp_control` control poll | back-to-back | sleep 5 s | sleep 3 s |
| QR scan / confirm | 800 ms between polls | scan: tolerate `qr_retry` non-zero codes | — |
| `getLoginInfo` | try auth host, then `wpa_chat` host | next host | — |
| History paging | 400 ms pause between pages | stop the run | — |
| Send with fallback (link, quote) | — | one fallback to plain text send, same `clientId` | — |

No generic automatic retry of failed sends: a send is retried only via an explicit fallback path or by the user, always reusing the original `clientId` ([message-identity.md](message-identity.md) §2).

## Open questions

- `[U]` Rate limiting: whether Zalo returns HTTP 429 or a specific `error_code`, and the required backoff.
- `[U]` Codes `102`, `19000` and any other codes from the brief: meaning unknown.
- `[U]` Full `-102` semantics (no live capture).
- `[U]` Socket-level error codes in frame envelopes and AUTHEN rejection codes.
- `[U]` Error code for a conflicting/duplicate login (kicked session).
