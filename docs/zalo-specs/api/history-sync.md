# History sync

How a client fetches message history beyond what the realtime channel delivers: group and "My Cloud" history paging, the legacy group-history endpoint, and the "sync messages from phone" (Đồng bộ tin nhắn từ điện thoại) flow, plus the rules for merging fetched pages with messages already held locally.

Evidence tags: `[O]` observed / live-verified, `[I]` implemented in the previous client without recorded live evidence, `[U]` unknown.

Related docs: [../security/payload-encryption.md](../security/payload-encryption.md) (session AES `encodeAES` / response decryption), [../auth/session.md](../auth/session.md) (`imei`, `computer_name`, `send2me_id`, common params), [../auth/service-discovery.md](../auth/service-discovery.md) (service map, host failover), [conversations.md](conversations.md) (`get-last-msgs` previews), [../realtime/websocket.md](../realtime/websocket.md) (push/pull queues, long-polling), [../client-rules/message-model.md](../client-rules/message-model.md) (message object fields, ID priority), [../client-rules/errors.md](../client-rules/errors.md).

---

## 1. Overview: where history comes from

| Conversation kind | Source of older messages | Notes |
|---|---|---|
| Group (`g<groupId>`) | Cloud-message API `{group_cloud_message}/api/cm/getrecentv2` + `/api/cm/getoldv2` (§3) | Primary paging path. `[I]` |
| Group (legacy) | `{group}/api/group/history` (§4) | Single page of N newest messages, no cursor. `[I]` |
| My Cloud (Cloud của tôi, the self conversation, id = `send2me_id`) | Same cloud-message API with `isOA: 1` (§3) | `[I]` |
| 1-1 (user) | **No REST paging endpoint known.** Only the latest-message preview (`get-last-msgs`, see [conversations.md](conversations.md)) and the realtime queues. | `[O]` the shipped PC bundle's REST command registry contains no "get message history" command; history otherwise lives in the sync-v2 worker. `[U]` how the official client pages older 1-1 history. |

`[I]` (project design note) the realtime channel and the preview endpoint only cover messages since the session started / roughly the last 50 messages, so paging is required to show anything older.

---

## 2. Conversation-id normalisation

- `[I]` Group ids may appear locally as `g<id>`, `G<id>`, `group_<id>` or bare `<id>`. Every history request takes the **bare numeric id**: strip a leading `group_`, then a leading `g`/`G`.
- `[I]` The My Cloud conversation id is `send2me_id`, returned in the login config (see [../auth/session.md](../auth/session.md)). It is a bare id, not prefixed.

---

## 3. Cloud-message history (groups and My Cloud)

`[I]` Code comments trace these to the PC bundle's `getCM` in the sync-v2 sub-worker, with command ids **12506 / 12507**, and state that cloud-message history is enabled for group conversations and for the `sendToMeId` conversation. Which id belongs to which path is not recorded. `[U]`

### Get recent cloud messages
- **Request:** `GET {group_cloud_message}/api/cm/getrecentv2?zpw_ver=<ver>&zpw_type=<type>&params=<enc>&nretry=0`  (default host: `https://tt-group-cm.api.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`), URL-encoded into `params`. No `signkey` is sent. `[I]`
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `groupId` | string | yes | Bare group id, or `send2me_id` for My Cloud |
  | `globalMsgId` | string (decimal) | yes | Cursor. For "newest", send the sentinel `"9223372036854775808"` (2^63). The previous client sends the sentinel when its cursor is empty or `"0"`. |
  | `count` | int | yes | Page size; previous client always uses `50` |
  | `msgIds` | array | yes | Always `[]` in the previous client; meaning unknown `[U]` |
  | `imei` | string | yes | Device id (`<imei>`) |
  | `isOA` | int | only for My Cloud | `1` when `groupId == send2me_id`; omitted otherwise |
  | `src` | int | yes | `1` for `getrecentv2` |
- **Response (`data` after decryption):** see "Response handling" below.
- **Errors:** generic envelope `error_code` (see [../client-rules/errors.md](../client-rules/errors.md)); no endpoint-specific codes recorded. `[U]`
- **Notes:** `[I]` the service key is `group_cloud_message` in `zpw_service_map_v3`; it is not in the client's verified host table, so the host from the login payload is always used, falling back to the default host above.

### Get older cloud messages (page backwards)
- **Request:** `GET {group_cloud_message}/api/cm/getoldv2?zpw_ver=<ver>&zpw_type=<type>&params=<enc>&nretry=0`  (default host: `https://tt-group-cm.api.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`); no `signkey`. `[I]`
- **Params (plaintext, before encryption):** identical to `getrecentv2` except:
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `globalMsgId` | string (decimal) | yes | The oldest server message id already held; the server returns messages older than it `[I]` (whether the cursor message itself is included is `[U]`) |
  | `src` | int | yes | `3` for `getoldv2` |
- **Response (`data` after decryption):** see below.
- **Errors:** as above.
- **Notes:** paging algorithm in §5.

### Response handling (both endpoints)
`[I]` The previous client accepts every one of the following shapes, which suggests (but does not prove) that several were seen:
1. Decrypt `data` with the session key if it is a base64 string (see [../security/payload-encryption.md](../security/payload-encryption.md)). The decrypted text is a full envelope with its own `error_code`.
2. If the resulting `data` is itself a **JSON string** starting with `{`/`[`, parse it again.
3. If `data` has an inner `data` object/array, descend into it.
4. Messages are either the node itself (array) or the concatenation of arrays under `msgs`, `groupMsgs` and `messages`.
5. Paging hints, when present (read only by one caller): `hasMore` (int, `0` = no older page) and `lastMsgId` (string or number; next cursor). `[I]`
6. Each element is a standard message object (see [../client-rules/message-model.md](../client-rules/message-model.md)). The destination is read from the first present of `idTo`, `toId`, `grid`, `groupId`; after prefix stripping, an element whose destination is non-empty, not `"0"`, and not this conversation's id **must be discarded** (the server can return messages of other conversations). `[I]`

---

## 4. Legacy group history

### Get group history
- **Request:** `GET {group}/api/group/history?zpw_ver=<ver>&zpw_type=<type>&params=<enc>`  (default host: `https://tt-group-wpa.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`). Signing tag recorded as **11240** (the command id). `[O]` per bundle trace quoted in code: the bundle builds `{grid, count}` → `encodeAES(JSON)` → `params`, and sends **no `signkey`**.
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `grid` | string | yes | Bare group id |
  | `count` | int | yes | Number of newest messages (previous client used 50 in the TUI, 10 in a probe tool) |
  | `imei` | string | no | **Not sent by the official bundle** `[O]`; the previous client's primary variant added it `[I]` and fell back to the exact `{grid, count}` form only on a transport error |
- **Response (`data` after decryption):** parsed with the same rules as §3 "Response handling". `[I]`
- **Errors:** none recorded. `[U]`
- **Notes:** no cursor field, so this cannot page backwards. Prefer §3 for groups. `[I]` Whether it still answers on current servers is `[U]`.

---

## 5. Paging algorithm (groups and My Cloud)

`[I]` Implemented in the previous client; the stop conditions are defensive because the server's paging contract is not confirmed.

```
cursor = oldest numeric server id stored locally for this conversation, else ""
repeat up to MAX_PAGES:
    if user cancelled: stop
    resp = getoldv2(groupId, globalMsgId = cursor or SENTINEL, count = 50)
    added = merge(resp)                      # §7
    if added == 0: stop                      # nothing older on the server
    persist page immediately                 # a cancel/crash loses nothing
    next = oldest message id in the merged set that is non-empty and all digits
    if next is missing or next == cursor: stop   # cursor did not move -> would loop forever
    if resp.hasMore == 0 (when present): stop
    cursor = next
    sleep ~400 ms
```

- **Cursor value** `[I]`: the id of the oldest message held, restricted to all-digit ids. Local, unsent echoes carry non-numeric ids (e.g. `out_*`) and must never be used as a cursor. The `lastMsgId` field of the response, when present and all digits, is preferred over the locally computed one by one caller.
- **Ordering** `[I]`: after every merge the conversation is re-sorted ascending by timestamp; the server's in-page order is not relied on (`[U]` what that order is).
- **Page size** `[I]`: 50. `[U]` server maximum.
- **Client policy, not server rules** `[I]`: a manual "sync history" run walks at most 20 pages (1 000 messages) per conversation and resumes from the stored oldest id on the next run; opening a group/My Cloud conversation fetches `getrecentv2` (cursor = sentinel) then up to 5 `getoldv2` pages. Only one bulk sync runs at a time.
- **Rate** `[I]`: ~400 ms between pages, ~150 ms between conversations. No rate-limit error has been recorded. `[U]`

---

## 6. Sync messages from phone (mobile → PC)

The official PC client can ask the user's phone to transfer its message history. The previous client implemented the request/cancel calls, but **never received or decoded a transfer payload**: there is no websocket command, push handler or decryption step for transferred data. What it called "phone sync" in practice was the composite refresh procedure in §6.4. The real delivery channel, payload format, encryption and compression are all unknown. `[U]`

### 6.1 Sync key
`[I]` Every request in this flow carries a `public_key`: 32 random bytes, standard base64 (44 chars). The same value is used for the request, the v2 request and the cancel of one sync session. Despite the name, the previous client generated no key pair, it is just random bytes. Whether the phone uses it to encrypt the transfer, and what the matching private material is, is `[U]`.

### Request mobile message pull
- **Request:** `GET {file}/api/message/pull_mobile_msg?zpw_ver=<ver>&zpw_type=<type>&params=<enc>`  (default host: `https://tt-files-wpa.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`); no `signkey`. Command id **12000** (bundle function `pullMobileMsg`). `[I]`
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `pc_name` | string | yes | Computer name, same value as login's `computer_name` |
  | `public_key` | string | yes | Sync key (§6.1) |
  | `from_seq_id` | int | yes | `0` |
  | `is_retry` | int | yes | `0` |
  | `min_seq_id` | int | yes | `0` |
  | `last_action_id` | int | yes | `0` |
  | `last_group_action_id` | int | yes | `0` |
  | `temp_key` | string | yes | `""` |
  | `imei` | string | yes | `<imei>` |
- **Response (`data` after decryption):** unknown; the previous client only checks `error_code == 0`. `[U]`
- **Errors:** none recorded. `[U]`
- **Notes:** `[I]` tried first as GET with host failover across `file` candidates; if that fails or `error_code != 0`, retried once as `POST` (form body `params=<enc>`, common params in the query) to the primary `file` host. Which method the server actually accepts is `[U]`. Meaning of the zeroed sequence / action-id fields (likely "resume from" markers) is `[U]`.

### Request transfer sync (v2)
- **Request:** `GET {file}/api/transfer-sync-v2/request-sync?zpw_ver=<ver>&zpw_type=<type>&params=<enc>`  (default host: `https://tt-files-wpa.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`); no `signkey`. Command id **12888** (bundle function `requestSyncMessage`). `[I]`
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `pc_name` | string | yes | Computer name |
  | `public_key` | string | yes | Sync key (§6.1) |
  | `version` | int | yes | `1` |
  | `is_retry` | int | yes | `0` |
  | `last_action_id` | int | yes | `0` |
  | `last_group_action_id` | int | yes | `0` |
  | `temp_key` | string | yes | `""` |
  | `imei` | string | yes | `<imei>` |
- **Response (`data` after decryption):** unknown. `[U]`
- **Errors:** none recorded. `[U]`
- **Notes:** same GET-then-POST fallback as above. `[I]` The previous client sends both 12000 and 12888 back-to-back with the same key, ignoring their results; whether the official client sends one or both (e.g. v2 replacing v1) is `[U]`.

### Cancel mobile message pull
- **Request:** `GET {file}/api/message/cancel_pull_mobile_msg?zpw_ver=<ver>&zpw_type=<type>&params=<enc>`  (default host: `https://tt-files-wpa.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`); no `signkey`. Command id **12700** (bundle function `cancelTransferMessage`). `[I]`
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `public_key` | string | yes | Sync key of the session being cancelled |
  | `imei` | string | yes | `<imei>` |
- **Response (`data` after decryption):** unknown. `[U]`
- **Errors:** none recorded. `[U]`
- **Notes:** `[I]` GET only, with host failover; no POST fallback. Whether it also cancels a v2 (12888) request is `[U]`.

### 6.2 Delivery of transferred data
`[U]` Not known. Candidates a re-implementer should investigate with a capture: a websocket push command, the long-polling message queue, or a file download on the `file` service (the endpoints live there). An earlier project design note planned support for `raw`/`deflate`/`gzip`/`zstd` payloads decrypted with AES-CBC/GCM, but no such codec was ever implemented or observed, so the format is entirely `[U]`.

### 6.3 When it runs
`[I]` The previous client starts it automatically right after a successful login (QR and password) and on user request; only one run at a time; a user "stop" sends the cancel call at the next phase boundary.

### 6.4 Composite refresh procedure used as "phone sync"
`[I]` Client policy, recorded because it is what produced visible results:
1. Generate a sync key; send 12000 then 12888 (results ignored). If cancelled now, send 12700 and stop.
2. Wait ~600 ms for queues to fill.
3. Drain the long-polling message queue (see [../realtime/websocket.md](../realtime/websocket.md)) for up to 5 batches; continue while `data.more != 0` or `data.resetLastActionId` is a non-empty value other than `"0"`. Merge messages and read-points.
4. Call `get-last-msgs` once with an empty map, then with all known conversation ids in chunks of 25 (~120 ms apart) (see [conversations.md](conversations.md)).
5. For every group and the My Cloud conversation: one `getrecentv2` page (cursor sentinel) then one `getoldv2` page from the oldest numeric id.
6. Resolve names, de-duplicate conversations, persist.

---

## 7. Merge / de-duplication rules

`[I]` Applied to every fetched page (cloud history, legacy history) against the messages already held (local store, realtime pushes, local echoes):

1. Discard elements addressed to another conversation (§3, step 6).
2. Match each incoming message to an existing one **only by identity** as defined in [../client-rules/message-model.md](../client-rules/message-model.md): exact non-empty server ids, or a client id bridging a local echo and its server copy. Text and timestamp never establish identity.
3. On a match: keep the existing row; merge reaction metadata; if the existing row has a local echo id (`out_*`) and the incoming one has a server id, adopt the server id; if the existing row lacks link-preview fields and the incoming one has them, copy them (and its non-empty text).
4. No match: append.
5. Re-sort the conversation ascending by timestamp; refresh the conversation preview from the newest message with non-empty text.
6. Persist with insert-or-ignore on `(conversation id, message id)`; a message without any id gets a synthetic `<conversationId>_<ts>` key locally. The literal placeholder text `sendBubbleMessage` is never stored.

Because merge is idempotent, pages may be re-fetched safely and arrive in any order relative to realtime pushes. `[I]`

---

## Open questions

- `[U]` Which message-id field is the correct `globalMsgId` cursor. The paging cursor is the stored message id, and the message model picks `msgId` before `cliMsgId` before `globalMsgId`; if a history element's `msgId` is not its global id, the cursor may be wrong. Needs a capture comparing the fields.
- `[U]` Whether `getoldv2` includes the cursor message itself, the server's maximum `count`, and the in-page ordering.
- `[U]` Meaning of `msgIds`, `src` values beyond 1/3, `nretry`, and `isOA` (why My Cloud is flagged as an "OA").
- `[U]` Mapping of command ids 12506 / 12507 to `getrecentv2` / `getoldv2`.
- `[U]` Any 1-1 history paging API (REST or sync-v2 binary).
- `[U]` Whether `/api/group/history` still works and its response shape; the code comment says "signing tag 11240" but also "no `signkey` on this path" for the same request.
- `[U]` Phone sync: response shapes of 12000/12888/12700, the correct HTTP method, whether both requests are needed, how and where the transferred data arrives, its encryption (role of `public_key`, `temp_key`) and compression. No live evidence that the phone actually transferred anything to the previous client.
- `[U]` Rate limits for bulk paging.
