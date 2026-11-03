# Session

What every request carries (common params, headers, cookies), how a session is set up with `getLoginInfo`, which values to persist for auto-login, how the device id is generated, and how to tell an expired session from a network failure.

Evidence tags: `[O]` observed/live-verified, `[I]` implemented in the previous client without recorded live evidence, `[U]` unknown.

## Common params

```
zpw_ver  = 694
zpw_type = 30
```

- Every authenticated REST call and the WebSocket URL carry `?zpw_ver=694&zpw_type=30`. That is the **entire** common object. Everything else is per-call. [O]
- Login-group endpoints carry the same values under other names: `client_version=694`, `type=30`. [O]
- **Version:** the bundle analysed (Zalo PC 26.10.11) defaults to `zpw_ver=693`. The current official desktop sends `694` on its general API, and the previous client used `694` everywhere. [O] A captured official desktop sent `zpw_ver=693&zpw_type=24` on the `voice_call` endpoints only. Whether the backend depends on that pair is unknown. See `call/` docs. [O]/[U] Make the version configurable. The bundle lets a stored `zpw_ver` override its default. [O]
- `zpw_type=31` is the embed-bundle flavour. Not used. [O]
- The bundle adds `ref=<page origin>` to the common params on cross-domain requests. [O] The previous client added `ref` only where a login endpoint needs it. [I]

## Headers

Sent on every request to a host equal to or ending in `zalo.me` / `zaloapp.com`:

| Header | Value | Evidence |
|---|---|---|
| `Origin` | `https://chat.zalo.me` | [O] |
| `Referer` | `https://chat.zalo.me/` | [O] |
| `User-Agent` | a desktop Electron UA, e.g. `Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) ZaloPC/26.10.11 Chrome/122.0.0.0 Electron/29.0.0 Safari/537.36` | [I] whether the server checks it is unknown |
| `Cookie` | `zpw_sek=<sek>; _zlang=vi` (once logged in) | [O] |
| `zpw_sek` | `<sek>` (non-standard header that repeats the cookie) | [I] whether it is needed is unknown |
| `Content-Type` | `application/x-www-form-urlencoded` on form POSTs | [O] |

The WebSocket handshake sends `Cookie`, `zpw_sek`, `Origin` and `User-Agent` the same way. [O]

Previous client timeouts: 10 s connect, 35 s total for regular calls (the total has to cover the long polls), 90 s for multipart uploads. [I]

## Cookies

- **`zpw_sek`** is the session token. **It gates all chat data.** Calls without it return `error_code 600 "zpw_sek not found or invalid"` (HTTP 200). [O]
- It is issued after QR confirmation or password login. On the QR path it arrives in a `Set-Cookie: zpw_sek=…` on the `pc-status` confirm response. See [qr-login.md](qr-login.md). [I]
- Cookies issued by Zalo live on `.zaloapp.com`, but many services live on `*.chat.zalo.me`. Send `zpw_sek` on **both** domain families: attach it to every service host from [service-discovery.md](service-discovery.md), or simply add the explicit `Cookie` header on every Zalo-host request. [O]
- Ignore a cookie value of `EXPIRED`. [I]
- `_zlang=vi` is the UI language cookie, always sent next to `zpw_sek`. [O]
- Other cookies the server may set at login (`Set-Cookie` names beyond `zpw_sek`) have not been listed. Keep a normal cookie jar so they are replayed automatically. [U]

Unauthenticated surface: only `getServerInfo`, `reqqr`, the QR polling URLs, `getCaptcha` and `loginByPasswv2`. Everything else needs `zpw_sek`. [O]

## Session setup: getLoginInfo

### getLoginInfo
- **Request:** `GET {auth}/api/login/getLoginInfo` (default host: `https://wpa.zaloapp.com`). Fallback host `https://wpa.chat.zalo.me`. [O]
- **Encoding:** ParamCipher v2, signkey tag `getlogininfo`. See [../security/param-cipher.md](../security/param-cipher.md). `nretry=0` is appended **after** signing. [O]
- **Params (plaintext, before encryption):**

  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `imei` | string | yes | device id |
  | `computer_name` | string | yes | device name |
  | `language` | string | yes | `vi` |
  | `ts` | int | [I] | Unix time, seconds |

- **Response (`data` after decryption):** fields read by the previous client: [O] for `zpw_enk` and the endpoint keys, [I] for the rest
  - `zpw_enk`: session AES key (base64), see [../security/payload-encryption.md](../security/payload-encryption.md). The bundle loads it with `setDecryptKey(data.zpw_enk)`. [O]
  - `uid`: own user id. The bundle also uses `dkey` as `UIN`. [O] The previous client fell back through `uid` → `userId` → `account.userId`/`account.uid` → `dkey`. [I]
  - `zpw_service_map_v3`, `zpw_chts_new`, `zpw_ctrs_new`, `zpw_ws`, `zpw_conf`: endpoints, see [service-discovery.md](service-discovery.md). [O]
  - `send2me_id`: id of the "my cloud" self-chat thread. [I]
  - `zpw_sek` or `account.zpw_sek`, if present. [I]
  - `account.display_name` / `display_name` / `settings.profile.name`. [I]
- **Errors:** inner `error_code` (after decryption) `0` on success. The previous client tried the auth host first, then the wpa-session host. It moved on to the next host on a transport error **or** a non-zero inner code. Comment: PC-flavour cookies live on `wpa.zaloapp.com` and web-flavour cookies on `wpa.chat.zalo.me`. [I]
- **Notes:** call it right after QR confirm or password login, with the `zpw_sek` cookie attached. It also works later as auto-login with a stored cookie. [O]

The own-profile call `GET {profile}/api/social/profile/me-v2?zpw_ver&zpw_type&params=<AES(zpw_enk, {"avatar_size":160,"imei":<imei>})>&os=8` returns `data.profile.{userId, displayName|zaloName, avatar|avt}`. The previous client used it to refresh identity and as the session-validity probe (below). [O] (it works live) Full schema is in `api/` docs.

## What to persist for auto-login

To restore a session without logging in again, persist per account: [I] (field set from the previous client; the `zpw_sek` + `zpw_enk` + `imei` core is [O])

| Value | Why |
|---|---|
| `imei` | Must stay the same for the session. It is inside ParamCipher `zcid` and many payloads. |
| `computer_name` | Sent alongside `imei`. |
| `zpw_sek` | Session cookie. **Without it the session cannot be restored.** |
| `zpw_enk` (or the parsed key bytes) | Session AES key. |
| own `uid`, display name, avatar, `send2me_id` | Identity for UI and message ownership. |
| Endpoint table (service-map hosts, long-poll hosts, `ws` list) | Saves a round trip. Refresh it from the next login payload. |
| "refused at <time>" marker | See expiry below. |
| Optional per-account proxy | Multi-account setups. [I] |

Treat this data as a credential. Store it with owner-only permissions and write it atomically. Never log `zpw_sek`, `zpw_enk` or `imei`. [I]

On restore, if the stored `imei` does not have the expected 69-char shape, generate a new one. That effectively invalidates the session. [I]

## Device identity (IMEI)

The official PC `imei` is: [O]

```
imei = uuid_v4() + "-" + md5_hex(user_agent)        # 36 + 1 + 32 = 69 chars
```

- `uuid_v4`: lowercase hex, version nibble `4`, variant nibble in `{8,9,a,b}`.
- `user_agent`: the official desktop UA string. The previous client hashed `Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Zalo/26.10.11 Chrome/122.0.6261.95 Electron/29.1.5 Safari/537.36`. [I] This is not the same UA it sent in HTTP headers. Whether the server compares the two is unknown. [U]
- The shape matters: the voice-call backend gave a degraded invite to a 24-hex homegrown id. [O]
- Generate it once per device or account and persist it. Do not regenerate it per run while a session is alive. [I]
- `computer_name`: a free-form device name (the previous client used the OS computer name, falling back to `DESKTOP`). [I]

## Expiry and re-login signals

| Signal | Verdict | Action |
|---|---|---|
| `error_code 0` | valid | — |
| `error_code 600` (`zpw_sek not found or invalid`) | **expired** [O] | Ask the user to log in again |
| `error_code -102` | **expired** [I] | Ask the user to log in again |
| Any other `error_code` (`111`, `114`, `18022`, …) | undetermined | Keep the session |
| Transport error, DNS, timeout, HTTP 4xx/5xx, unparsable body | undetermined (offline) | Keep the session, run from local data, retry later |

Rules: [I] (the codes behind them are [O] for `600`)

- **Only `600` and `-102` invalidate a session.** Never delete stored credentials over a network failure or an unrelated code (#21).
- Validate at startup and on each manual refresh by calling `me-v2` (above), decrypting it, and classifying the inner `error_code`.
- When the server refuses: keep the stored credentials, record a "refused at <epoch>" marker, and send the user to login. Replace the stored session only after a fresh login succeeds, and clear the marker then. On the next start, a session carrying the marker goes straight to login without testing it again.
- Logout is local only: clear the `zpw_sek`, the key, the identity and the cookie jar. Server endpoints `/api/login/logOut` and `/api/v2/login/logOut` exist (cmd `11135`) but were never implemented. [U]

## Open questions

- [U] Full list of cookies set at login and whether any besides `zpw_sek` is needed.
- [U] Whether the `zpw_sek` request header and the `User-Agent` matter to the server.
- [U] The meaning of `-102` (no recorded message text).
- [U] Whether `zpw_ver=693` is still accepted (version pinning), and why voice-call endpoints use `693`/`24`.
- [U] The exact `getLoginInfo` response schema (the field fallbacks above are defensive guesses).
- [U] Server-side logout request shape.
- [U] Whether `ts` in `getLoginInfo` is required.
