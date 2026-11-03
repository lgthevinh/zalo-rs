# Realtime events and command IDs

Catalogue of the WebSocket command IDs and the event payloads delivered over the socket and the HTTP long-pollers, with what each carries and how a client should react. Transport, framing and decryption are in [websocket.md](websocket.md).

Related docs: [../client-rules/message-model.md](../client-rules/message-model.md), [../client-rules/message-identity.md](../client-rules/message-identity.md), [../client-rules/unread.md](../client-rules/unread.md), [../client-rules/reactions.md](../client-rules/reactions.md), [../call/](../call/) (VoIP control events), [../api/history-sync.md](../api/history-sync.md).

Naming: `<cmd>_<subCmd>` identifies a queue (e.g. `603_1`). Directions: S→C server to client, C→S client to server.

---

## 1. WebSocket command IDs

Names come from the bundle's command enum (`A = {AUTHEN:1, PING:2, …, CTRL_*}`) `[O]`. Semantics beyond the name are `[I]` unless stated.

| cmd | Name | Dir | Carries / purpose | Client reaction | Tag |
|---|---|---|---|---|---|
| 1 | `AUTHEN` | S→C | First frame: `{key, encrypt, data}`; `data` → `{error_code, data:{ctrl_qCmds, qCmds, …}}` | Store key; on `error_code 0` pull listed queues; else close + reconnect | `[O]` |
| 2 | `PING` (sub 1 `KEEP_CONNECTION`) | C→S, S→C | `{eventId: <unix ms>}`; pong echoes `eventId` | Send every ~15 s after auth | `[O]` frame, `[I]` interval |
| 4 | `PING_ACTIVE` (sub 2 `KEEP_ACTIVE`) | C→S | `[U]` presumably activity/presence keepalive | Not sent by previous client | `[I]` |
| 501 | `PUSH_MSG_1_1` | S→C | Live 1-1 message(s) | Merge into conversation by identity | `[I]` |
| 521 | `PUSH_MSG_GROUP` | S→C | Live group message(s) | Merge into group conversation | `[I]` |
| 510 | `PULL_MSG_1_1` | C→S / S→C | Pull request / page of offline 1-1 messages | Merge; follow `data.more == 1` | `[I]` |
| 511 | `PULL_MSG_GROUP` | C→S / S→C | Pull request / page of offline group messages | Same | `[I]` |
| 513, 514, 515, 517, 518 | `PULL_MSG_*` | C→S / S→C | Message-bearing pull queues; `[U]` exact meaning of each | Treat as message pages | `[I]` |
| 533 | `HAS_MSG` | S→C | `data.queueId`, `data.ids` — notification that messages are waiting | Send `570_0` with those IDs | `[I]` |
| 534 | `PUSH_MISS_MSG` | S→C | `[U]` "missed messages" push | Treat as message page | `[I]` |
| 570 | `GET_MSG` | C→S / S→C | Request `{queueId, ids, lastId:"1", reqId}` / response with messages | Merge messages | `[I]` |
| 601 | `CTRL_PUSH_CTRL` | S→C | Live control events (`controls[]`, VoIP invites, mute) | Dispatch by `act_type` (§3) | `[O]` name, `[I]` content |
| 602 | `CTRL_ACTION` | S→C | Control action events | Dispatch by `act_type` | `[I]` |
| 603 | `CTRL_PULL_CTRL` | C→S / S→C | Pull queue for control events (seen as `603_0`, `603_1`) | Dispatch by `act_type` | `[I]` |
| 604 | `CTRL_PULL_VOIP` | C→S / S→C | Pull queue for VoIP events (`604_0`) | Dispatch VoIP | `[I]` |
| 610 | — | — | Appears in `ctrl_qCmds` (`610_1`) in a test fixture; `[U]` meaning | Pull only if listed | `[U]` |

Rules `[I]`:
- Pull only the queues listed in AUTHEN; pulling an unowned queue closes the socket with `3002`.
- `[I]` A captured official desktop session subscribed to `603_1`, `603_0` and `604_0`; full media-carrying VoIP invites only arrived there.
- A frame may carry `error_code`; log non-zero values. `[U]` meaning of per-frame error codes.
- Unknown `cmd` values must be logged, not treated as errors.

---

## 2. Message-bearing payloads

The previous client parsed every message-bearing frame (socket cmds 501, 521, 510, 511, 513–518, 534, 570 and `lp_chat` responses) with one tolerant "preview merge" routine `[I]`:

- Locate the list: the payload's `data` (may be a JSON **string** that must be parsed again, and may be nested once more under `data.data`), then any of `msgs` (1-1), `groupMsgs` (group), `pageMsgs`, `conversations`, `threads`, `allmsgs`, or a top-level array.
- Each entry may itself be a message, or a wrapper with `messages[]`, `msgs[]`, `groupMsgs[]`, `lastMsg{}`, `message{}`.
- Each message is parsed per [../client-rules/message-model.md](../client-rules/message-model.md) and routed to a conversation (§2.1), then merged by identity ([../client-rules/message-identity.md](../client-rules/message-identity.md)).

### 2.1 Conversation routing — `[O]` (sync worker)

```
if message is in a group bucket (groupMsgs) or isGroup == true/1, or has grid/groupId:
    thread = "g" + (grid ?? groupId ?? idTo)            # bundle: msgKey = GROUPID_PREFIX + idTo
elif idTo == "0" and uidFrom == "0":
    thread = self userId                                  # bundle: idTo = "" + this.userId  (My Cloud / self)
else:
    thread = (idTo == "0") ? uidFrom : idTo               # bundle _filterDuplicatedMessages
```

- `uidFrom == "0"` means **sent by the current account** (outgoing) `[O]`.
- `[I]` A message from a peer to us has `idTo` = our uid or `"0"`; the conversation is `uidFrom`.
- `[I]` Never auto-create a group conversation from `clearUnreads` only; disbanded groups linger there.

### 2.2 Message events by `msgType`

| `msgType` | Event | Client reaction | Tag |
|---|---|---|---|
| `webchat`, `chat.text`, … | New message | Insert or merge | `[O]` names |
| `chat.undo` | Recall ("thu hồi") of an earlier message; content references the target (`globalMsgId` / `msgId` / `cliMsgId`) | Mark the **target** as recalled; do not insert a new row | `[I]` |
| `chat.delete` | Delete (archive code 33) | `[U]` semantics (delete-for-me vs. admin delete) | `[O]` name |
| `chat.reaction` | Reaction event | Apply per [../client-rules/reactions.md](../client-rules/reactions.md) | `[O]` name |
| `chat.list.action` | OA action list | Render as action/system | `[O]` name |
| others | see message-model | | |

`[I]` The literal content `"sendBubbleMessage"` is a placeholder; skip it.

---

## 3. Control events (`act_type` / `act`)

Control payloads (socket cmds 601/602/603/604 and `lp_control`) carry `controls: [ { "content": { "act_type": <string>, "act": <string>, "data": <object or JSON string> } } ]`; `data` frequently arrives as a JSON **string** and must be parsed `[I]` (`_processCtrlMessages` `[O]`).

| `act_type` | `act` | `data` fields | Client reaction | Tag |
|---|---|---|---|---|
| `voip` | `request`, `group_request` | `callId`, `uidN`, `ts`, `params` (JSON string: `sessId`, `rtpIP`, `codec`, `extendData`, …) | Incoming call; see [../call/](../call/) | `[I]` (socket-only `[O]`) |
| `voip` | `answer` and others | call status (e.g. `status: 5`) | Update call state | `[I]` |
| `mute` | `set` / `unset` (or `action` 1 = set) | `toId`/`toid`/`id`, `muteType` (1 user, 2 group), `duration` (seconds; `-1` indefinite; `0` = not muted), `muteMode` (1 = muted), `startTime`/`systemTime` (unix s) | Update mute; expiry = `startTime + duration` | `[I]` (`_processMuteConversations` `[O]`) |
| `typing` | `typing` | `[U]` | Show typing indicator | `[U]` (only in a synthetic test) |

Mute rule `[I]`: muted iff (`act == "set"` or `action == 1`) and `muteMode == 1` and `duration != 0`. Group thread when `muteType == 2` or the id starts with `g`/`group_`.

---

## 4. Other event payloads in poll responses

| Key | Where | Meaning | Reaction | Tag |
|---|---|---|---|---|
| `clearUnreads` (`clearUnreadInfo`, `statusMap.clearUnreads`) | `lp_chat`, `get-last-msgs` | Server read cursors `{idTo, lastMsgId, isGroup, type, sct}` | Advance read cursor ([../client-rules/unread.md](../client-rules/unread.md)) | `[I]` |
| `reacts`, `reactGroups` | `lp_chat` | Reaction events for 1-1 / groups | [../client-rules/reactions.md](../client-rules/reactions.md) | `[I]` |
| `more`, `lastActionId`, `queueStatus`, `resetLastActionId` | all pollers / pulls | Paging and cursor | [websocket.md](websocket.md) §2.4, §4 | `[I]` |

---

## 5. Events not yet decoded

All `[U]`:
- Seen / delivered receipts from peers (and the corresponding "I have read" receipt the client should send).
- Typing indicator payload shape and its command.
- Group events: member join/leave/kick, rename, avatar change, admin change, group disband.
- Friend events: request received/accepted/removed, block.
- Pin / unpin conversation, label, hide-conversation sync from other devices.
- Profile / avatar / online-status updates.
- Forced logout / login-from-another-device.
- Phone → PC history transfer delivery (see [../api/history-sync.md](../api/history-sync.md) §6.2).

A re-implementation should record every `cmd`, `act_type`, `act` and `msgType` it does not recognise (sanitized) to close these gaps.

## Open questions

- `[U]` Exact meaning of pull queues 513, 514, 515, 517, 518, 534 and control queue 610.
- `[U]` Whether 501/521 bodies use the same `msgs`/`groupMsgs` container keys as `lp_chat`.
- `[U]` The full `act_type` vocabulary on 601/602/603.
- `[U]` Everything in §5.
