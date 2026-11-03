# Conversations: list sources, previews, polling, pin, unread mark, mute, hide, delete

How a client builds the conversation list, receives new messages over HTTP long-polling, and changes per-conversation settings (pin, unread mark, mute, hide, delete) on Zalo's servers.

Evidence tags: `[O]` observed (live-verified, or read from the shipped official Zalo PC bundle); `[I]` implemented in the previous client without recorded live evidence; `[U]` unknown.

Common params (`zpw_ver`, `zpw_type=30`), session-AES encoding of `params`, response decryption and tolerant `data` parsing are as in [contacts.md](contacts.md#conventions-shared-by-every-call-in-this-doc) and [../security/payload-encryption.md](../security/payload-encryption.md). Message parsing is in [../client-rules/message-model.md](../client-rules/message-model.md); client-side merge rules are in [../client-rules/threads.md](../client-rules/threads.md).

## Conversation ID forms

| Context | User (1-1) | Group |
|---|---|---|
| Thread key stored by the client | `<uid>` | `<grid>` (+ `is_group` flag) [I] |
| `get-last-msgs` key | `<uid>_0` | `<grid>_1` [I] |
| Pin list (`pinconvers`) | `u<uid>` | `g<grid>` [I] |
| Mute snapshot / mute events | `<uid>` | `g<grid>` or `<grid>` with `muteType=2` [I] |
| Most write calls (`toid`, `grid`, `thread_id`) | bare `<uid>` | bare `<grid>` [I] |

Before sending, strip a leading `group_`, then `g`/`G` (and `u` for the pin call). [I]

---

## 1. Where the conversation list comes from

There is no single "list all conversations" REST call. [O] The list is assembled from:

| Source | Gives | Doc |
|---|---|---|
| Friend list `getfriends` | every friend as a 1-1 thread (name, avatar) | [contacts.md](contacts.md#get-friends) |
| Group list `getlg/v4` + `getmg-v2` | every group (name, avatar, roster) | [groups.md](groups.md) |
| "My Cloud" thread | the user's own storage conversation | [§1.1](#11-my-cloud-conversation) |
| `get-last-msgs` | latest message(s) per recent thread, including non-friends | [§2](#2-last-messages-preview) |
| Long-polling / websocket | new messages; creates threads for unknown senders | [§3](#3-polling-for-new-messages), [../realtime/websocket.md](../realtime/websocket.md) |
| Local store | cached threads + history for instant first paint | [../client-rules/threads.md](../client-rules/threads.md) |

[O] The official client's registry also has these bootstrap calls; the previous client did **not** implement them: `/api/preloadconvers/get-last-seen` (chat service, cmd `12009`), `/api/hiddenconvers/get-all` (conversation, cmd `10048`), `/api/archivedchat/list` (conversation, cmd `12076`). Their shapes are [U].

### 1.1 My Cloud conversation

- The login config (`getLoginInfo`) carries `send2me_id`: the thread ID of the user's "My Cloud" (`Cloud của tôi`) conversation. [I] See [../auth/session.md](../auth/session.md).
- The client inserts it at the top of the list as a 1-1 thread named `"Cloud của tôi (Lưu trữ)"` and always sorts it first. [I]
- Its history comes from the cloud-message endpoints (`/api/cm/getrecentv2`, `/api/cm/getoldv2` on the `group_cloud_message` service) with `groupId = send2me_id` and `isOA = 1`; see [history-sync.md](history-sync.md). [I]
- [O] (sync worker) A message with `idTo == "0"` **and** `uidFrom == "0"` is a message to self; the official client rewrites `idTo` to the user's own ID. The previous client routed such messages to the My Cloud thread. [I]

---

## 2. Last messages (preview)

### Get last messages
- **Request:** `GET {conversation}/api/preloadconvers/get-last-msgs?zpw_ver=…&zpw_type=30&params=…` (cmd `10053`) (default host: `https://tt-convers-wpa.chat.zalo.me`)
- **Encoding:** session AES (`zpw_enk`); plain JSON `params` as fallback if no key. [O] shape (official `getLastMessagesForPreview`), [I] fallback.
- **Params (plaintext, before encryption):**

  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `threadIdLocalMsgId` | string (JSON-encoded object) | yes | `{}` = "all recent conversations"; or `{"<uid>_0":"1", "<grid>_1":"1", …}` for specific threads |
  | `imei` | string | yes | `<imei>` |

- **Response (`data` after decryption):** the previous client accepted any of these shapes [I]:
  - `data` is a JSON string → parse it; `data.data` nested → descend;
  - an array of entries; or an object with any of the lists `msgs`, `groupMsgs` (all entries are group messages), `pageMsgs`, `conversations`, `threads`, `allmsgs`;
  - `clearUnreads`: array of `{idTo | threadId | userId, isGroup}`;
  - if none of the lists is present, every object value of `data` is treated as an entry.

  Each entry may contain messages in `messages[]`, `msgs[]`, `groupMsgs[]` (group), `lastMsg{}`, `message{}`, or the entry itself is a message (has `content`, `msg` or `msgId`).
- **Errors:** `600` without a valid `zpw_sek`. [O]
- **Notes:**
  - [O] The official client sends `{}` on preload to fetch all recent previews without overflowing the URL.
  - [I] The previous client sends specific keys only for ≤ 25 threads per call; above 25 it sends `{}`. The value `"1"` is a placeholder; its meaning (local message ID?) is [U]. An earlier note described the argument as an array of `{threadId, localMsgId}` — the two descriptions disagree [U].
  - [I] Refresh strategy: one call with `{}`, then calls for all known threads in chunks of 25 (groups keyed `<grid>_1`).
  - [I] `clearUnreads` entries create a 1-1 thread named `"Người dùng <uid>"` if unknown; group entries there are **ignored** (disbanded groups linger in this list).

### Routing a message to a thread  [O] rules from the official sync worker, [I] fallbacks

```
route(entry, msg, inGroupBucket, myCloudId):
  isGroup = inGroupBucket or msg.isGroup or entry.isGroup        # isGroup: true or 1
  # 1. explicit group id
  for o in [msg, entry]:
     if o.grid or o.groupId (non-empty, != "0"): return (strip "group_" prefix, group)
  if isGroup:
     id = first of idTo, toid, toId, threadId, conversId on msg then entry
     if id non-empty and != "0": return (id, group)
  # 2. 1-1
  from = first of uidFrom, fromUid, fromId, senderId ;  to = first of idTo, toid, toId
  if to in ("", "0") and from in ("", "0") and myCloudId: return (myCloudId, user)   # message to self
  if from not in ("", "0") and (to in ("", "0") or to == myCloudId): return (from, user)  # incoming
  if to not in ("", "0"): return (to, user)                                          # outgoing
  if from not in ("", "0"): return (from, user)
  return first non-"0" of threadId, conversId, userId, friendId, id  (isGroup as computed)
```

- [O] Official dedup key: `isGroup ? msgKey : (idTo == "0" ? uidFrom : idTo)`, where a group's `msgKey` = group prefix + `idTo`.
- A message routed to an unknown thread creates it with a placeholder name (`"Nhóm <id>"` / `"Người dùng <id>"`), to be resolved later. [I]

---

## 3. Polling for new messages

### Long-poll messages (`getChatFromPolling`)
- **Request:** `GET {zpw_chts_new[0]}/?zpw_ver=…&zpw_type=30&params=<cursor>&ts=<unix seconds>` (cmd `99100`) [O] (official sync worker). Host is the first entry of `zpw_chts_new` from the login/server-info payload (a `p<N>-msg-chat` host, server-assigned; previous fallback `https://p4-msg-chat.zaloapp.com`). See [../auth/service-discovery.md](../auth/service-discovery.md).
- **Encoding:** plain — the cursor is **not** encrypted. [O]
- **Cursor format:** [O]
  ```
  params = lastActionId + "," + lastActionIdOther + "," + urlencode( base64(JSON(queues)) + ",1" )
  queues = {"1":{"lastId":"<id>","ids":[…]}, "2":{…}, "3":{…}}       # MSG queue IDs 1,2,3 (qosId 91021)
  ```
  Initial state: `lastActionId = lastActionIdOther = "1"`, each queue `{"lastId":"1","ids":[]}`. Keys are serialized in order `"1","2","3"`, fields in order `lastId`, `ids`. [I] ordering.
- **Response (`data` after decryption; may be nested in `data.data`):**

  | Field | Meaning |
  |---|---|
  | `lastActionId`, `lastActionIdOther` | new cursor values (numeric strings) |
  | `queueStatus` | `{ "<queueId>": {lastId, ids[]} }` |
  | `resetLastActionId` | non-zero = server demands a cursor reset |
  | `more` | `1` = more batches waiting |
  | `msgs`, `groupMsgs`, … | messages, same shapes as `get-last-msgs` |

  An HTTP 200 with an **empty body** is a keep-alive timeout = "no data". [O] (official `processEmptyData`)
- **Cursor update rules:** [I] except where marked
  1. If `resetLastActionId` is non-zero: set `lastActionId` and every queue's `lastId` to it, set `lastActionIdOther = "1"`, clear all `ids`; stop.
  2. `lastActionId` / `lastActionIdOther`: take the numeric max of old and new (ignore `"0"`).
  3. For each queue in `queueStatus` **that is one of 1, 2, 3** (ignore others such as 7, 8, 15, 16 — adding socket-only queues makes the server reject later polls): `lastId = max(old, new)`; `ids` = new IDs ≤ `lastId`, sorted descending, de-duplicated, at most 10, then reversed to ascending. [O] for the "max 10 ≤ lastId" rule (official `getLastActionIdsArray`).
- **Errors:** non-zero `error_code` aborts the batch; `600`-class session errors mean the session expired (see [../client-rules/errors.md](../client-rules/errors.md)). Other errors are reported and retried on the next tick. [I]
- **Timing:** [I]
  - Client-side timeout 12 s (matching the official long-poll window [O]); a timeout counts as an empty response.
  - After each response, if `more == 1` or a reset was received, poll again immediately, at most 3 batches per tick.
  - Ticks every 1.2 s; a tick is skipped while a previous poll is still in flight or when logged out.
  - The cursor is reset to the initial state on logout / new QR login; a full refresh starts from a fresh cursor and then replaces the shared one.

### Control long-poll
- A second long-poll on `zpw_ctrs_new[0]` (`p<N>-ctl-chat`) carries control events (VoIP signalling, mute changes). It is specified with calls in [../call/](../call/) and [../realtime/websocket.md](../realtime/websocket.md). Mute events from it are parsed as in [§6.3](#63-realtime-mute-events). [I]

### Realtime vs polling
- [O] An early plan chose "skip websocket, poll `get-last-msgs` every N s".
- [I] The previous client ended up running **all** of: the websocket (instant messages + controls, see [../realtime/websocket.md](../realtime/websocket.md)), the message long-poll above every 1.2 s, and the control long-poll; `get-last-msgs` and cloud-message fetches are only used on startup, manual refresh and when opening a thread. Messages are deduplicated by identity, so receiving the same message from several channels is harmless (see [message-identity.md](../client-rules/message-identity.md)).

---

## 4. Pinned conversations

### List pinned
- **Request:** `GET {conversation}/api/pinconvers/list?zpw_ver=…&zpw_type=30` (cmd `12120`) [O] (registry)
- **Encoding:** plain — no `params` sent by the previous client. [I]
- **Response:** [U] — never parsed by the previous client.
- **Notes:** [I] Pins were kept locally only; the server pin list was never read back. See Open questions.

### Pin / unpin
- **Request:** `POST {conversation}/api/pinconvers/updatev2?zpw_ver=…&zpw_type=30` (cmd `12122`, official `updatePinnedConversationsV2`); body `params=…`
- **Encoding:** session AES (`zpw_enk`)
- **Params (plaintext):**

  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `actionType` | int | yes | `1` = pin, `2` = unpin |
  | `conversations` | string[] | yes | `["u<uid>"]` or `["g<grid>"]` |
  | `tab` | int | yes | `0` [I] — presumably the main tab [U] |

  No `imei`. [I]
- **Response:** success envelope; unused. [I]
- **Errors:** [U]
- **Notes:** fire-and-forget; the local state is changed first. [I]

---

## 5. Unread mark

### Mark as unread / clear mark
- **Request:** `POST {conversation}/api/conv/addUnreadMark?…` (cmd `12013`) or `POST {conversation}/api/conv/removeUnreadMark?…` (cmd `12014`) (official `addMarkAsUnread` / `removeMarkAsUnread`); body `params=…`
- **Encoding:** session AES (`zpw_enk`). The encrypted object is a **wrapper** whose single field `param` is itself a JSON **string**:
  ```
  encrypt( { "param": JSON.stringify(inner) } )
  ```
- **Inner params:** [I]

  | Call | Field | Type | Meaning |
  |---|---|---|---|
  | add | `convsGroup` | string[] | bare grids to mark |
  | add | `convsUser` | string[] | bare uids to mark |
  | add | `imei` | string | `<imei>` |
  | remove | `convsGroup`, `convsUser` | string[] | as above |
  | remove | `convsGroupData`, `convsUserData` | array | sent empty `[]`; meaning [U] |

- **Response:** success envelope; unused. [I]
- **Errors:** [U]
- **Notes:** "Marked unread" is a flag separate from the unread message count; it shows a badge without a number. Clearing it happens when the user opens the thread or picks "mark as read". Unread counting itself is in [../client-rules/message-model.md](../client-rules/message-model.md). [I] Receiving other devices' unread marks was not implemented. [U]

---

## 6. Mute (notifications off)

### 6.1 Set / clear mute
- **Request:** `POST {profile}/api/social/profile/setmute?zpw_ver=…&zpw_type=30` (cmd `11288`, official `setMuteConversation`); body `params=…`
- **Encoding:** session AES (`zpw_enk`)
- **Params (plaintext):**

  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `toid` | string | yes | bare `<uid>` or `<grid>` |
  | `duration` | int | yes | seconds of mute, or `-1` = until turned back on (see below) |
  | `action` | int | yes | `1` = mute, `2` = unmute |
  | `startTime` | int | yes | now, **Unix seconds** |
  | `muteType` | int | yes | `1` = user, `2` = group |
  | `imei` | string | yes | `<imei>` |

- **Duration semantics (as offered by the official UI):** [I]

  | Option | `duration` |
  |---|---|
  | For 1 hour | `3600` |
  | For 4 hours | `14400` |
  | Until 8:00 AM | seconds from now until the next 08:00; if that is < 600 s, add 24 h |
  | Until turned back on | `-1` |
  | Unmute | `action = 2`, `duration = -1` (value sent by the previous client) |

  Expiry = `startTime + duration`. `duration = 0` means "not muted". [I]
- **Response:** success envelope; unused. [I]
- **Errors:** [U]

### 6.2 Get mute snapshot
- **Request:** `GET {profile}/api/social/profile/getmute?zpw_ver=…&zpw_type=30&params=…` (cmd `11280`, official `getMuteConversations`)
- **Encoding:** session AES (`zpw_enk`)
- **Params (plaintext):** `imei` = `<imei>`.
- **Response (`data` after decryption; may be a JSON string, may be nested in `data.data`):**

  | Field | Meaning |
  |---|---|
  | `chatEntries[]` | muted 1-1 conversations |
  | `groupChatEntries[]` | muted groups |

  Entry fields: `id` (uid, or grid possibly prefixed `g`), `muteMode` (`1` = muted), `duration` (seconds, `-1` = indefinite, `0` = not muted), `startTime` (or `systemTime`), Unix seconds. [I]
- **Interpretation:** [I]
  ```
  for each entry:
    if muteMode present and muteMode != 1: not muted
    if duration == 0: not muted
    if duration < 0: muted indefinitely
    else until = startTime + duration; muted iff until > now
  ```
  A response missing either array is not a valid snapshot and must be ignored. A valid snapshot is **authoritative**: any conversation not listed is unmuted.

### 6.3 Realtime mute events
- Delivered inside websocket control frames (cmd `601`/`602`/`603`) or the control long-poll; see [../realtime/websocket.md](../realtime/websocket.md). [I] (official `_processMuteConversations`)
- Shape: `controls[]` (or a bare array / single object) of items whose `content` (or the item itself) is:
  ```
  { "act_type": "mute", "act": "set" | "unset",
    "data": { toId|toid|id, muteType, duration, startTime|systemTime, muteMode, action } }   # data may be a JSON string
  ```
- Rules: [I]
  - group iff `muteType == 2` or the ID starts with `g`/`group_`;
  - `act = "set"` → muted iff `muteMode == 1` and `duration != 0` (defaults: `muteMode` 1, `duration` -1); `act = "unset"` → unmuted; any other `act` → use `action == 1` as "set";
  - muted with `duration < 0` → indefinite; else `until = startTime + duration` (default start = now); if `until <= now` → unmuted.

### 6.4 Bidirectional sync procedure  [I]
1. On startup and on every refresh, after messages are loaded: call **getmute** and replace the local mute state with the snapshot.
2. When the user mutes/unmutes locally: update the UI immediately, call **setmute**, then call **getmute** and replace local state with the snapshot (server is the source of truth).
3. Apply realtime mute events (§6.3) as they arrive, so changes made on the phone or official PC show up live.
4. Locally, a mute with an expiry stops applying once `now >= until` without any server call.

---

## 7. Hide conversation

### Hide / unhide
- **Request:** `POST {conversation}/api/hiddenconvers/add-remove?zpw_ver=…&zpw_type=30` (cmd `10049`, official `updateItemHiddenChat`); body `params=…`
- **Encoding:** session AES (`zpw_enk`)
- **Params (plaintext):**

  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `add_threads` | string (JSON-encoded array) | yes | threads to hide: `[{"thread_id":"<id>","is_group":0|1}]`, or `"[]"` |
  | `del_threads` | string (JSON-encoded array) | yes | threads to unhide, same element shape, or `"[]"` |
  | `imei` | string | yes | `<imei>` |

- **Response:** success envelope; unused. [I]
- **Errors:** [U]
- **Notes:** [I] The official hidden-chat feature is PIN-protected; the previous client did not send or verify any PIN and kept a local hidden set (hidden threads reappear when searched by name). Reading the server's hidden list (`/api/hiddenconvers/get-all`) was not implemented [U].

---

## 8. Delete conversation history (for me)

### Delete conversation
- **Request:**
  - 1-1: `POST {chat}/api/message/deleteconver?zpw_ver=…&zpw_type=30` (cmd `11778`, official `deleteOneOneConversationV2`)
  - group: `POST {group}/api/group/deleteconver?zpw_ver=…&zpw_type=30` (cmd `11835`, official `deleteGroupConversationV2`)

  body `params=…`
- **Encoding:** session AES (`zpw_enk`)
- **Params (plaintext):**

  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `toid` (1-1) / `grid` (group) | string | yes | bare conversation ID |
  | `cliMsgId` | string | yes | ID of the newest message to delete up to; fallback: now in ms |
  | `conver` | object | yes | `{}` [I] — meaning [U] |
  | `onlyMe` | int | yes | `1` = delete only on my side |
  | `imei` | string | yes | `<imei>` |

- **Response:** success envelope; unused. [I]
- **Errors:** [U]
- **Notes:** [I] The client removes the thread and its local history first, then fires the request. The previous client passed the newest message's *local ID* (usually the server `msgId`) as `cliMsgId`, not its client message ID — which one the server expects is [U].

## Open questions

- [U] `threadIdLocalMsgId` value semantics: object `{"<id>_<0|1>": "1"}` (previous client) vs array of `{threadId, localMsgId}` (earlier note). Which one does the server honour, and what does the value mean?
- [U] Which of the many response shapes of `get-last-msgs` / long-poll actually occur (`msgs`, `groupMsgs`, `pageMsgs`, `conversations`, `threads`, `allmsgs`); the parser was written to tolerate all of them.
- [U] Response shape of `/api/pinconvers/list`; whether the client should read pins from the server on startup (the previous client never did, so pins made elsewhere were not shown).
- [U] Shapes of `/api/preloadconvers/get-last-seen`, `/api/hiddenconvers/get-all`, `/api/archivedchat/list`.
- [U] Error codes of pin / unread mark / mute / hide / delete calls.
- [U] Meaning of `tab` (pin), `convsGroupData`/`convsUserData` (unread mark), `conver` (delete).
- [U] Whether `cliMsgId` for delete must be a client message ID or a server message ID.
- [U] "Until 8:00 AM" contradiction: the previous client computed the next 08:00 from **UTC** day boundaries (`(86400 - now % 86400 + 8·3600) % 86400`), i.e. 08:00 UTC = 15:00 Vietnam time. The intended meaning is surely local 08:00; what the official client sends is unverified.
- [U] Mute map key mismatch in the previous client: the snapshot keyed groups as `g<grid>` while group threads were stored under bare `<grid>`, so applying the snapshot could mark muted groups as unmuted. A rewrite must use one canonical ID form.
- [U] Hidden chats: PIN handling of the official feature and how hidden state is synced across devices.
- [U] Whether long-poll queues other than 1, 2, 3 are ever valid for HTTP polling (7, 8, 15, 16 were seen in `queueStatus`).
- [U] `zpw_ver` value: code used `694`, earlier notes and the bundle default record `693`.
