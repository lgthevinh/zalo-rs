# Realtime transport: control WebSocket and HTTP long-polling

How a client receives messages and control events in real time: the `zpw_ws` WebSocket (connection, handshake, binary frame codec, payload decryption, queue pulls, keepalive, reconnect) and the two HTTP long-polling receivers (`lp_chat`, `lp_control`) that act as a fallback and as the only path the previous client used for some data.

Related docs: [events.md](events.md) (what each command ID carries), [../auth/session.md](../auth/session.md) (`zpw_sek`, common params), [../auth/service-discovery.md](../auth/service-discovery.md) (`zpw_ws`, `zpw_chts_new`, `zpw_ctrs_new`), [../security/payload-encryption.md](../security/payload-encryption.md) (session AES used on long-poll responses), [../client-rules/errors.md](../client-rules/errors.md), [../api/history-sync.md](../api/history-sync.md).

Source of the protocol facts: static decoding of the Zalo PC 26.10.11 renderer bundle (`getSocketUrl`, `_connectSocket`, `_send`, `_getHeader`, `_processEventBlob`, `_decrypt`, `_decompress`, `doGetOffline`, `_onPong`) plus the sync worker (`getChatFromPolling`, `getLastActionIds`, `QueueControl`). Facts read from the bundle are tagged `[O]`; behaviour only present in the previous client is `[I]`.

---

## 1. Endpoints

| Transport | Source of host | Default (used only when login config lacks it) |
|---|---|---|
| WebSocket | login/server-info config key `zpw_ws` (array of full `wss://` URLs) | `wss://ws1-msg.chat.zalo.me` … `wss://ws8-msg.chat.zalo.me` |
| Message long-poll (`lp_chat`) | `zpw_chts_new[0]` | `https://p4-msg-chat.zaloapp.com` |
| Control long-poll (`lp_control`) | `zpw_ctrs_new[0]` | `https://p4-ctl-chat.zaloapp.com` |

- `[O]` The server returns full `wss://` URLs in `zpw_ws`; the client never builds them from a template. Keep only entries that start with `wss://` or `ws://`; strip a trailing `/`.
- `[O]` The `p<N>` host index is server-assigned per session and varies; never hardcode `p4`/`p5`.
- `[U]` How many `zpw_ws` entries are returned. One earlier note says "ws{1..8}, 6 endpoints"; the previous client defaulted to all eight.
- `[I]` `zpw_chts_new` / `zpw_ctrs_new` are long-polling hosts, **not** REST hosts. An older client stored them in the REST `chat`/`conversation` slots and broke REST calls; keep them separate.

---

## 2. WebSocket connection

### 2.1 URL

```
url = zpw_ws[i]
if url has no path after the authority: append "/"
url += "?" + (existing query of zpw_ws[i] + "&" if any) + "zpw_ver=<ZPW_VER>&zpw_type=<ZPW_TYPE>" + "&t=" + <unix ms>
```

- `[O]` Bundle: `Q[i] + "?" + commonParams`, then `&t=<Date.now()>`. `Q` is the `zpw_ws` list. `binaryType = "arraybuffer"`.
- `[I]` Common params sent by the previous client: `zpw_ver=694`, `zpw_type=30` (see [../auth/session.md](../auth/session.md)).
- `[I]` The `zlpws` front end answers `501 Not Implemented` to a request line with an empty path (`GET ?zpw_ver=…`); always send an explicit `/`. (Recorded as a server reply in a code comment; no capture on file.)

### 2.2 Handshake headers

| Header | Value | Tag |
|---|---|---|
| `Cookie` | `zpw_sek=<zpw_sek>; _zlang=vi` | `[I]` |
| `zpw_sek` | `<zpw_sek>` (same value as the cookie) | `[I]` |
| `Origin` | `https://chat.zalo.me` | `[I]` |
| `User-Agent` | Zalo PC Electron UA, e.g. `Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) ZaloPC/26.10.11 Chrome/122.0.0.0 Electron/29.0.0 Safari/537.36` | `[I]` |

- `[O]` `zpw_sek` is the session credential that gates every authenticated surface ([../auth/session.md](../auth/session.md)).
- `[U]` Whether `zpw_sek` as a separate header is required or only the cookie; the previous client sent both.
- `[U]` Whether `Origin` / `User-Agent` are checked.

### 2.3 Session establishment (AUTHEN)

1. `[O]` The **server speaks first**: right after the WebSocket opens it sends a frame with `cmd = 1` (`AUTHEN`). The client sends nothing until then.
2. `[O]` AUTHEN envelope (JSON body): `{ "key": <base64>, "encrypt": <0..3>, "data": <string> }`.
   - `key` base64-decodes to the AES-GCM key for this connection (16 or 32 bytes seen as supported; `[U]` which length is actually issued).
   - `data` is decoded with the `encrypt` scheme (§3.3) using that key.
3. `[I]` Decoded `data` is a JSON response envelope `{ "error_code": 0, "data": { … } }`. Any `error_code ≠ 0` → treat the connection as rejected, close, and reconnect with backoff (§2.6).
4. `[O]` `data.ctrl_qCmds` (and `[I]` `data.qCmds`) list the queues this connection may pull: arrays of `{ "cmd": <u16>, "subCmd": <u8> }`. Example shape (synthetic): `ctrl_qCmds: [{cmd:610,subCmd:1},{cmd:603,subCmd:1}]`, `qCmds: [{cmd:511,subCmd:1}]`.
5. `[O]` For each listed queue (deduplicated, in order) send an initial pull (§2.4).
6. `[I]` Only pull queues the server listed. Pulling a queue the shard does not own (example: `604_0`) makes the server close the socket with close code `3002 NOT_FOUND_WORKER` (recorded in code comments; no capture on file).

Other AUTHEN `data` fields: `[U]` not documented.

### 2.4 Pulling queues

Initial pull, one per queue — `[O]` (`doGetOffline`):

```
frame(version=1, cmd=<queue cmd>, subCmd=<queue subCmd>,
      body={"first": true, "reqId": "req_<n>", "lastId": 1, "preIds": []})
```

- `reqId` is `"req_"` + a per-connection counter starting at 0, incremented for every request frame. `[O]` format, `[I]` counter start.
- `[U]` Whether `lastId` should be a persisted cursor (resuming offline delivery across restarts) rather than `1`. The previous client always started at `1`.

Follow-up page — `[I]`: when a decoded payload has `data.more == 1`, pull the same `cmd`/`subCmd` again:

```
lastId = data.lastActionId
      ?? data.queueStatus["<cmd>_<subCmd>"].lastId
      ?? 1
preIds = data.queueStatus["<cmd>_<subCmd>"].ids ?? []
body   = {"first": false, "reqId": "req_<n>", "lastId": lastId, "preIds": preIds}
```

"Has message" notification — `[I]`: when a frame with `cmd = 533` (`HAS_MSG`) decodes to a payload with `data.queueId` and `data.ids`, request those messages:

```
frame(version=1, cmd=570, subCmd=0,
      body={"queueId": <data.queueId>, "ids": <data.ids>, "lastId": "1", "reqId": "req_<n>"})
```

The answer arrives as `cmd = 570` frames ([events.md](events.md)).

### 2.5 Keepalive

- `[O]` Application ping: `frame(version=1, cmd=2 (PING), subCmd=1 (KEEP_CONNECTION), body={"eventId": <unix ms>})`. The server's pong echoes `eventId` (`_onPong` matches it).
- `[I]` Interval: every 15 s, only after AUTHEN succeeded. The previous client did not verify pongs. `[U]` The official client's interval and its pong-timeout policy.
- `[I]` A second constant exists: `cmd = 4` (`PING_ACTIVE`) with `subCmd = 2` (`KEEP_ACTIVE`). `[U]` When the official client sends it (likely user-activity / presence); the previous client never sent it.
- `[I]` Reply to WebSocket-level ping control frames with a pong carrying the same payload. Ignore text frames.

### 2.6 Reconnect policy

`[I]` Previous client behaviour (no official policy decoded — `[U]`):

```
backoff = 2 s
endpoint_index = 0
loop:
    connect to zpw_ws[endpoint_index mod len]; run until close/error
    if consumer gone: stop
    endpoint_index += 1          # rotate shard on every reconnect (helps after 3002)
    sleep(backoff)
    backoff = min(backoff * 2, 30 s)
```

- Quirk to avoid: the previous client never reset `backoff` after a healthy connection, so every later reconnect waited 30 s. Reset it after a successful AUTHEN.
- `[I]` Supervisor rules: start the socket only when logged in with a non-empty `zpw_sek`; when `zpw_sek` changes (re-login) tear down and open a new socket; when logged out drop it.
- `[I]` The WebSocket ran **in parallel** with HTTP long-polling (§4); events from both must be deduplicated by message identity ([../client-rules/message-identity.md](../client-rules/message-identity.md)).

### 2.7 Multiple connections / duplicate login

- `[U]` Whether a second connection for the same session (another tab/device, or two client instances) kicks the first, and which close code or frame signals it. Not observed.
- `[U]` Whether the server sends a "logged in elsewhere" / forced-logout command over the socket. Not decoded. A re-implementation should log unknown `cmd` values and close codes so this can be captured.
- `[I]` Known close code: `3002 NOT_FOUND_WORKER` (shard does not host this session/queue) → rotate to the next `zpw_ws` entry.

---

## 3. Frame codec

### 3.1 Binary layout — `[O]`

Every WebSocket binary message is one frame:

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | `version` | client sends `1` |
| 1 | 2 | `cmd` | unsigned 16-bit **little-endian** |
| 3 | 1 | `subCmd` | unsigned 8-bit |
| 4 | n | body | UTF-8 JSON; may be empty (n = 0) |

Encode: `[version] + le16(cmd) + [subCmd] + utf8(json)`. Decode: reject frames shorter than 4 bytes; empty body → null payload; unparsable JSON → drop the frame (log it).

### 3.2 Client → server bodies

`[I]` Plain JSON, not encrypted or compressed (pull, get-msg and ping bodies above). `[U]` Whether any client→server command requires encryption with the AUTHEN key.

### 3.3 Server → client envelope and `encrypt` schemes — `[O]`

Server bodies are an envelope `{ "encrypt": <n>, "data": <string>, … }` (AUTHEN also carries `key`; `error_code` may be present).

| `encrypt` | Meaning | Decode of `data` |
|---|---|---|
| 0 | plain | the envelope itself is the payload (for AUTHEN, `data` is the plaintext JSON string) |
| 1 | compressed | base64-decode → inflate |
| 2 | encrypted + compressed | percent-decode → base64-decode → AES-GCM decrypt → inflate |
| 3 | encrypted | percent-decode → base64-decode → AES-GCM decrypt |

AES-GCM blob layout — `[O]`:

```
blob = iv (16 bytes) | aad (16 bytes) | ciphertext || tag (16 bytes)
plaintext = AES-GCM-Decrypt(key = AUTHEN key, nonce = iv (16-byte nonce), aad = aad, ciphertext||tag)
```

- Minimum blob length 48 bytes. AES-128 or AES-256 by key length.
- Note the **16-byte** GCM nonce (not the common 12); implementations must use a GCM mode that accepts a 16-byte IV (GHASH-derived J0).
- Percent-decoding (`decodeURIComponent`) must tolerate raw `+`, `/`, `=` (only `%XX` sequences are decoded).

Inflate — `[O]`: for `encrypt` 2/3 the server sends **gzip** (`1f 8b 08 …`). The bundle calls a generic helper that sniffs the container. Decode as gzip when the magic matches, otherwise zlib, and fall back to raw deflate. Older captures decoded as zlib.

After decoding, the payload is JSON. `[I]` If decryption or parsing fails, the previous client forwarded the raw envelope instead of dropping it; a re-implementation should log and drop.

---

## 4. HTTP long-polling receivers

`[O]` The sync worker (`sync-v2-sub-worker`) has two pollers built by the same class:

| Poller | Bundle definition | Host | Command | Queue IDs |
|---|---|---|---|---|
| Message (`lp_chat`) | `type: "MSG", qosId: 91021, queueIds: [1,2,3]` | `zpw_chts_new[0]` | `getChatFromPolling`, cmd `99100` | `"1"`, `"2"`, `"3"` |
| Control (`lp_control`) | control poller | `zpw_ctrs_new[0]` | `getControllFromPolling`, cmd `99101` | `"4"`, `"5"` `[O]`; previous client also added `"14"`, `"17"`–`"20"` `[I]` |

`[O]` HTTP long-polling only carries durable actions. Live VoIP invites (`act:"request"`) are socket-only; a polling-only callee sees `act:"answer" status:5` stubs. See [../call/](../call/).

### Poll message / control queue
- **Request:** `GET {zpw_chts_new[0]}/?zpw_ver=694&zpw_type=30&params=<cursor>&ts=<unix seconds>` (default host: `https://p4-msg-chat.zaloapp.com`); control poller identical on `{zpw_ctrs_new[0]}` (default host: `https://p4-ctl-chat.zaloapp.com`).
- **Encoding:** plain — `params` is **not** AES-encrypted; it is the cursor string below. Response `data` may be a base64 session-AES blob (decrypt with the session key, [../security/payload-encryption.md](../security/payload-encryption.md)). No signkey. Session cookie headers as for all REST calls.
- **Params (the `params` cursor string):**

  ```
  params = lastActionId + "," + lastActionIdOther + "," +
           urlencode( base64( JSON(queues) ) + ",1" )
  queues = { "<queueId>": { "lastId": "<id>", "ids": ["<id>", ...] }, ... }
  ```
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `lastActionId` | numeric string | yes | global action cursor; initial `"1"` |
  | `lastActionIdOther` | numeric string | yes | secondary cursor; initial `"1"` |
  | `queues[q].lastId` | numeric string | yes | per-queue cursor; initial `"1"` |
  | `queues[q].ids` | array of numeric strings | yes | acknowledged action IDs (see below); initial `[]` |
  | trailing `,1` | literal | yes | `[U]` meaning |
  `[O]` format (`getLastActionIds`).
- **Response (`data` after decryption):** may be nested once more under `data.data`. Keys used:
  - `msgs` (1-1), `groupMsgs` (groups), `pageMsgs` — arrays of wire messages ([../client-rules/message-model.md](../client-rules/message-model.md)). `[I]`
  - `clearUnreads` — server read cursors ([../client-rules/unread.md](../client-rules/unread.md)). `[I]`
  - `reacts`, `reactGroups` — reaction events ([../client-rules/reactions.md](../client-rules/reactions.md)). `[I]`
  - `controls` — control events (control poller) `[{content:{act_type, act, data}}]` ([events.md](events.md)). `[I]`
  - `lastActionId`, `lastActionIdOther`, `queueStatus: {"<q>": {lastId, ids:[…]}}`, `more` (0/1), `resetLastActionId`. `[I]`
- **Errors:** `error_code` inside the decrypted envelope; `600` = session gone ([../client-rules/errors.md](../client-rules/errors.md)).
- **Notes:**
  - `[O]` HTTP 200 with an **empty body** means the long-poll window expired with nothing new (`if ("" === e.data) return processEmptyData()`); treat as success with no data.
  - `[O]` Window ~12 s (`sync-v2-sub-worker`); the previous client aborted after 12 s and treated the timeout as an empty result.
  - Cursor update `[I]` (mirrors `QueueControl.getLastActionIdsArray` `[O]`):
    ```
    if resetLastActionId is a positive number:
        lastActionId = reset; lastActionIdOther = "1"
        for each queue: lastId = reset; ids = []
        return
    lastActionId      = max(lastActionId, data.lastActionId)          # ignore "0"
    lastActionIdOther = max(lastActionIdOther, data.lastActionIdOther)
    for q, st in data.queueStatus:
        if q not a configured queue: skip                              # see below
        lastId = max(lastId, st.lastId)                                # ignore "0"
        ids = st.ids filtered to <= lastId, sorted desc, dedup, first 10, then ascending
    ```
    Compare IDs numerically with arbitrary precision (≥ 128-bit or string compare), never floats.
  - `[I]` The server keeps re-delivering a control-queue entry until the client echoes its action ID back in `ids` (`getIdsForParam()` upstream). For the control poller, an empty `ids` in `queueStatus` clears the stored `ids`.
  - `[I]` Message poller: **only** update the configured queues `1`,`2`,`3`. Adding socket-only queues seen in `queueStatus` (`"15"`, `"16"`, `"7"`, `"8"`) to the request made `lp_chat` reject subsequent requests. The control poller, by contrast, adopted every queue in `queueStatus`.
  - `[I]` Drain: after a response with `more == 1` or a non-zero `resetLastActionId`, poll again immediately (previous client: up to 3 rounds per tick; 5 during phone sync).
  - `[I]` Cadence: message poll triggered every 1.2 s, skipped while one is in flight. Control poll runs back-to-back; on error sleep 5 s; while logged out sleep 3 s.
  - `[I]` First control poll with cursor `"1"` returns backlog; drop old VoIP invites and keep only `act` `request`/`group_request` whose `data.ts` is within 45 s (seconds or ms; values < 10¹⁰ are seconds).

---

## Open questions

- `[U]` Full AUTHEN `data` schema, and the semantic of `ctrl_qCmds` vs `qCmds`.
- `[U]` Official ping interval, pong timeout, and when `PING_ACTIVE (4/2)` is sent.
- `[U]` Official reconnect/backoff policy and close-code handling beyond `3002`.
- `[U]` Duplicate-login / kick behaviour and its signalling (command or close code).
- `[U]` Whether `lastId` in socket pulls must be persisted across restarts to avoid missing or replaying offline messages; whether ack (`preIds`) semantics match the long-poll `ids` echo.
- `[U]` Meaning of the trailing `,1` in the long-poll cursor and of `lastActionIdOther`.
- `[U]` Whether the official client runs WebSocket and long-polling simultaneously or only falls back to polling.
- `[U]` AES key length issued in AUTHEN `key`.
- Contradiction: an earlier note claimed frames are JSON `{name, data}` and that the handshake was not decoded. The binary 4-byte-header codec above (decoded later from the same bundle) supersedes it.
