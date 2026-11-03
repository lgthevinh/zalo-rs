# Service discovery

Zalo spreads its API over many hosts. The server tells the client which host serves each service through `getServerInfo` and the login payload (`getLoginInfo`). This doc covers the request, the service-map keys, default hosts, the WebSocket host list, and how to pick and fail over between hosts.

Evidence tags: `[O]` observed/live-verified, `[I]` implemented in the previous client without recorded live evidence, `[U]` unknown.

## Get server info

### getServerInfo
- **Request:** `GET {auth}/api/login/getServerInfo` (default host: `https://wpa.zaloapp.com`)
- **Encoding:** plain query, signed with signkey tag `getserverinfo` (see [../security/signkey.md](../security/signkey.md)). Params go **inline in the query string**, not inside `params=`. [O]
- **Params (plaintext):**

  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `imei` | string | yes | device id, see [session.md](session.md#device-identity-imei) |
  | `type` | string/int | yes | `30` (= `zpw_type`) |
  | `client_version` | string/int | yes | `694` (= `zpw_ver`) |
  | `computer_name` | string | yes | device display name |
  | `sp_mtn` | int | yes | `-1` |
  | `bkt` | int | yes | `-1` |
  | `signkey` | string | yes | `md5("zsecure" + "getserverinfo" + sorted values of the six fields above)` |

- **Response `data`:** plain JSON (not encrypted). Top-level keys seen: `zpw_chts_new`, `zpw_ctrs_new`, `zpw_service_map_v3`, `zpw_conf`, `zpw_ws`, `setttings` (sic, triple `t`), `qos_setting`. [O] `setttings.authen.trusted_device` controls QR flavour (see [qr-login.md](qr-login.md)). [O] The bundle reads `enable_encrypt_param` from this config. Whether that field appears in the live response has not been recorded. [U]
- **Errors:** `18022 Invalid Signature` when unsigned or wrongly signed. [O]
- **Notes:** works without a session. Call it once at startup, before login. The previous client did this and ignored any failure. [I] The bundle uses a 5-second timeout and caches the result. [O] One doc table lists `https://wpa.chat.zalo.me` as the host for `getServerInfo`. The previous client called it on the auth host `https://wpa.zaloapp.com`. [U] Which host is canonical for an authenticated web session is not settled.

The same endpoint keys (`zpw_chts_new`, `zpw_ctrs_new`, `zpw_ws`, `zpw_service_map_v3`) also come back in the `getLoginInfo` `data`. Apply them again after every login. [O]

## Payload keys

| Key | Shape | Meaning |
|---|---|---|
| `zpw_service_map_v3` | object `{service_key: [url, …] \| url}` | REST host per service. **This** is the live key. [O] |
| `zpw_service_map_new` | same | Older name read by the bundle. **Absent** from live responses. Reading only this key silently leaves every host at its default. [O] Accept it as a fallback only. [I] |
| `zpw_chts_new` | `[url, …]` or url | HTTP long-polling **message** receiver, e.g. `https://p<N>-msg-chat.zaloapp.com`. [O] |
| `zpw_ctrs_new` | `[url, …]` or url | HTTP long-polling **control** receiver, e.g. `https://p<N>-ctl-chat.zaloapp.com`. [O] |
| `zpw_ws` | `[wss-url, …]` | WebSocket endpoints, full `wss://` URLs. [O] |
| `zpw_conf` | object | Client config. Contents not documented here. [U] |

Each service-map value can be an **array or a single string**. Strip trailing `/`, drop empty entries, and keep the order. Take the first entry as primary and the rest as candidates. Values without a scheme get `https://` added. [I]

Bundle mapping (`setEndpoints(chts, ctrs, mapNew, conf, ws)`): `chts → chat`, `ctrs → conversation`, `mapNew.{friend, profile, group, file, label, alias, board, sticker}`. `mapNew.conversation` overrides the conversation host only when present. [O] The previous client treated `chts`/`ctrs` as the long-polling hosts. REST `chat` and `conversation` come from the service map. [O]

## Service keys and hosts

Two tables follow. The **bundle defaults** are what the PC bundle uses before it hears from the server, with `CONFIG_DOMAIN = zaloapp.com`. The **live-verified** hosts answered `error_code 0` in a QR-authenticated Zalo Web session on 2026-10-21. [O]

| Service key | Live-verified host (2026-10-21) | Bundle default | Main endpoints (details in `api/` docs) |
|---|---|---|---|
| — (auth) | `https://wpa.zaloapp.com` | `https://wpa.zaloapp.com` (`authDomain`) | `/api/login/reqqr`, `/api/login/loginByPasswv2`, `/api/login/getServerInfo`, `/api/login/getLoginInfo` |
| — (wpa session) | `https://wpa.chat.zalo.me` | — | `/api/login/getLoginInfo` (web-session cookies) |
| `chat` | `https://tt-chat3-wpa.api.zaloapp.com` | `https://chat-wpa.zaloapp.com` | `/api/message/sms`, `/api/message/sticker`, `/api/message/quote` |
| `conversation` | `https://tt-convers-wpa.chat.zalo.me` | `https://convers-wpa.api.zaloapp.com` | `/api/preloadconvers/get-last-msgs`, `/api/pinconvers/list`, `/api/hiddenconvers/get-all` |
| `friend` | `https://tt-friend-wpa.chat.zalo.me` | `https://friend-wpa.zaloapp.com` | `/api/social/friend/getfriends` |
| `profile` | `https://tt-profile-wpa.chat.zalo.me` | `https://profile-wpa.zaloapp.com` | `/api/social/profile/me-v2`, `/api/social/profile/getminiprofiles` |
| `group` | `https://tt-group-wpa.zaloapp.com` | `https://group-wpa.zaloapp.com` | `/api/group/getlg/v4`, `/api/group/sendmsg`, `/api/group/history` |
| `file` | `https://tt-files-wpa.zaloapp.com` | `https://files-wpa.zaloapp.com` | uploads |
| `sticker` | `https://tt-sticker-wpa.chat.zalo.me` | `https://sticker-wpa.zaloapp.com` | `/api/message/sticker/*` |
| `alias` | `https://tt-alias-wpa.chat.zalo.me` | `https://alias-wpa.api.zaloapp.com` | contact aliases |
| `label` | `https://label-wpa.chat.zalo.me` | `https://label-wpa.api.zaloapp.com` | conversation labels |
| `reaction` | `https://reaction.api.zaloapp.com` | — | reactions |
| `emoji_status` | `https://emoji-status.chat.zalo.me` | — | status emoji |
| `zcloud` | `https://zcld.chat.zalo.me` | — | cloud storage |
| `voice_call` | `https://voicecall-wpa.chat.zalo.me` | `https://voicecall-wpa.zaloapp.com` | `/api/voicecall/*` |
| `group_cloud_message` | (server-assigned) | `https://group-cm.api.zaloapp.com` | `/api/cm/getoldv2`, `/api/cm/getrecentv2` |
| (long-poll msg) `zpw_chts_new` | server-assigned, e.g. `https://p5-msg-chat.zaloapp.com` | — | message long-poll |
| (long-poll ctl) `zpw_ctrs_new` | server-assigned, e.g. `https://p5-ctl-chat.zaloapp.com` | — | control long-poll |

Bundle domain derivation: `CONFIG_DOMAIN` defaults to `zaloapp.com` (launch param `zdmapi` can override it). `authDomain` defaults to `https://wpa.{CONFIG_DOMAIN}` (install-config param `zauthdm`, which must start with `https`, can override it). The cookie domain is `.{CONFIG_DOMAIN}`, and requests are sent with credentials, so auth is cookie-based. [O] Old notes also mention `vlogin.zaloapp.com` as an auth domain. [U]

Other bundle defaults that have no service key (not used by the previous client): `board-wpa`, `groupboard-wpa.api`, `friendlan-wpa.api`, `media-wpa`, `spcontact-wpa.api`, plus `https://id.zaloapp.com/account/login|logout` and `https://chat.zaloapp.com/login`. [O] An older client set seeded `group_cloud_message` with `https://tt-group-cm.api.zaloapp.com`. [I]

### Two host flavours

The PC bundle defaults to `*.zaloapp.com`. The live Web/QR session serves `friend`, `profile`, `conversation`, `sticker`, `alias`, `label` and `voice_call` from `*.chat.zalo.me`. Calling the wrong flavour returns **nginx HTTP 404**. [O] The server has also been seen handing out the stale `*.zaloapp.com` hosts for `friend`/`profile` in `zpw_service_map_v3` (#22). [O] Do not blindly trust the server map for these keys: use the candidate list below.

### Host index

The `p<N>` / `ws<N>` / `tt-chat<N>` index is **server-assigned per session** and changes between sessions and even between responses (`p4`, `p5`, `p7` have been seen). [O] Never hardcode an index for the long-poll hosts. Always take them from the latest payload. [O]

## Host selection and failover

Each service gets an ordered candidate list with duplicates removed: [I]

1. The host that last succeeded for this service in the current process (in memory only).
2. The live-verified host from the table above, if one exists.
3. All hosts from `zpw_service_map_v3[key]`, in server order.
4. The host persisted from the previous session.
5. For `friend` only: the `profile` host.

Try the candidates in order. **Move to the next one only on a transport failure (connect, DNS, timeout) or HTTP 404.** Stop right away on any response that carries a non-zero `error_code` (`600`, `-102`, or a business code). A different host will not fix those. Remember the winning host for the rest of the process. [I] The 404 trigger comes from the wrong-flavour behaviour above. [O]

Hosts with no verified entry (long-poll `chts`/`ctrs`, `ws`, `group_cloud_message`) are taken from the latest server payload on every login. [I]

## WebSocket host list

- `zpw_ws` gives full `wss://` URLs, e.g. `wss://ws<N>-msg.chat.zalo.me`. Six endpoints were seen in one response. [O] Fallback when none is known: `wss://ws1-msg.chat.zalo.me` … `wss://ws8-msg.chat.zalo.me`. [I]
- Keep only entries that start with `wss://` or `ws://`. [I]
- Connect URL: `<ws-url>` + `/` if the URL has no path (the front end answers `501` to an empty path), then `?zpw_ver=694&zpw_type=30&t=<unix-ms>`. Existing query parameters on the URL are kept. [O]
- On disconnect or error, move to the next endpoint in the list (index + 1, wrapping) and reconnect with exponential backoff: 2 s, doubling, capped at 30 s. A `3002 NOT_FOUND_WORKER` close means the shard does not host this session, so rotate. [I] Full transport details are in `realtime/` docs.

## Open questions

- [U] Canonical host for `getServerInfo` once a session exists (`wpa.zaloapp.com` vs `wpa.chat.zalo.me`).
- [U] Whether `enable_encrypt_param` is present in the live `getServerInfo` response.
- [U] Contents of `zpw_conf` and `qos_setting`.
- [U] Whether the server's `*.zaloapp.com` `friend`/`profile` hosts ever work for Web-flavour sessions, or are always stale.
- [U] An older doc mapped `zpw_chts_new[0]` to "chat (send/history)". The previous client used it only as the message long-poll host and sent messages through the `chat` service-map host.
