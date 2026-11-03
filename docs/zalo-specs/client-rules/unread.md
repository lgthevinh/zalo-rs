# Unread state

How a client computes unread counts, mention badges and mute state from server read cursors and conversation settings, and which requests read or change them. Core principle: **unread state follows server read cursors, never the size of a history batch.** If no authoritative read point is known, the unread count is unknown, not "everything loaded".

Related docs: [message-model.md](message-model.md), [message-identity.md](message-identity.md), [../realtime/events.md](../realtime/events.md), [../realtime/websocket.md](../realtime/websocket.md) (long-poll), [../api/conversations.md](../api/conversations.md) (`get-last-msgs`, mute), [errors.md](errors.md).

---

## 1. Server read cursors (`clearUnreads`)

`[I]` Read points arrive in the `lp_chat` long-poll response and the `get-last-msgs` preview response, under any of: `clearUnreads`, `clearUnreadInfo`, or `statusMap.clearUnreads`.

Entry shape:

| Key | Type | Meaning |
|---|---|---|
| `idTo` | string/number | Conversation id (peer uid, or group id without prefix) |
| `lastMsgId` | numeric string/number | Newest message ID that is read in that conversation |
| `isGroup` | bool or 0/1 | Group conversation |
| `type`, `sct` | number | Sub-chat cursor markers |

Rules `[I]` (the official desktop consumer ignores subchat cursors `[O]`):
- Skip entries with `type == 2` that carry `sct` (sub-chat cursors, `sct` 1/2/3).
- Skip entries whose `lastMsgId` is non-numeric or `0`, or whose `idTo` is empty/`"0"`.
- Group conversation id = `"g" + idTo`.
- Apply as `advance_seen(lastMsgId)` on the matching conversation; never create a group conversation from a cursor (disbanded groups linger there).

---

## 2. ID ordering

Message IDs are compared as arbitrary-size unsigned decimals `[I]`:

```
numeric(id): id must be all ASCII digits and non-empty; strip leading zeros ("" -> "0")
compare(a, b): compare by length, then lexicographically; undefined if either is non-numeric
```

No floating point; IDs above 2⁶³ must compare exactly.

---

## 3. Per-conversation state machine

State `[I]`:

| Field | Meaning |
|---|---|
| `last_seen_id` | Read cursor; `none` = no authoritative read point yet |
| `unread_ids` | Set of incoming message IDs newer than the cursor |
| `mention_ids` | Subset of `unread_ids` that mention me |
| `muted`, `muted_until` | Mute state (unix s; none = indefinite) |
| `marked_unread` | Manual "mark as unread" flag |

Operations:

```
advance_seen(point):
    if point not numeric: ignore
    if last_seen_id is none or point >= last_seen_id:      # never regress
        last_seen_id = point
        drop unread_ids and mention_ids <= point

observe(messages):                     # only for NEW live messages or a read-cursor snapshot
    if last_seen_id is none: return    # do not fabricate unread from history
    for m in messages:
        if incoming and not system and counts_unread(m) and m.id > last_seen_id:
            unread_ids.add(m.id)       # set → replays deduplicate

read_visible(messages):                # local foreground reading
    advance_seen(max numeric id among visible messages)
    marked_unread = false
    # sends NO receipt (see §6); off-screen newer unread IDs remain
```

Merging state from different sources `[I]`:
- A **fresh network** snapshot may advance the cursor and add unread IDs above it; it may also replace mute/marked-unread settings when it carries them.
- A cached/history copy may not change state, except to fill an empty state that has no cursor yet.

### `counts_unread` — `[O]`

The shipped JS rule is `paramsExt && countUnread !== 1` → **does not count**. Equivalently a message counts toward unread when:

```
paramsExt is absent, or paramsExt is falsy (null, false, 0, ""), or paramsExt.countUnread == 1 (strict numeric)
```

---

## 4. Mentions — `[O]`

From the official notification worker: a message mentions me when any `mentions[].uid` equals my uid or equals `MENTION_ALL_ID = "-1"`. Archive rows carry the list at `meta.mentionsList` (opaque/obfuscated uids are decoded upstream; `"-1"` stays literal).

Classification `[I]`: `mention`, `no-mention`, or `unknown`.
- `unknown` if own uid is unknown/`"0"`, the mentions field is missing or not an array, an entry lacks `uid`, or an archive uid is non-numeric (cannot be compared without the decoder).
- Outgoing messages (sender `"0"`) → `no-mention`.
- Only update `mention_ids` for messages already in `unread_ids`. `unknown` never erases a previously verified mention. Outgoing/system/non-counting messages are removed.

---

## 5. Badge rendering — `[I]`

```
has_badge = count > 0 or marked_unread
if not has_badge: none
if muted (and muted_until is none or in the future) or count == 0: dot (no number)
group and count > 5: "5+"
count > 99: "99+"
else: count
```

---

## 6. Requests

### Mark-read / seen receipt
- `[U]` Not implemented by the previous client; reading locally sent no receipt and did not change the account. The endpoint and payload the official client uses to report "seen" (and "delivered") are unknown. Incoming seen/delivered events from peers are also `[U]` ([../realtime/events.md](../realtime/events.md) §5).

### Get manual unread marks
- **Request:** `GET {conversation}/api/conv/getUnreadMark?zpw_ver=694&zpw_type=30&params=<enc>` (default host: `https://tt-convers-wpa.chat.zalo.me`)
- **Encoding:** session AES (`zpw_enk`) of `{}`; response `data` decrypted with the session key. Command `12011` `[I]`.
- **Params (plaintext, before encryption):** none (empty object).
- **Response (`data` after decryption):** may be a JSON string; `{ convsGroup: [{id, cliMsgId, fromUid}], convsUser: [{id, cliMsgId, fromUid}] }`. Both arrays must be present, else the snapshot is incomplete and must be ignored. Group id → `"g" + id`.
- **Errors:** non-zero `error_code` at either envelope level → ignore snapshot.
- **Notes:** read-only. A complete snapshot sets `marked_unread` true for listed conversations and false for all others. `[I]` (implemented, never wired into the previous UI)

### Add / remove manual unread mark
- **Request:** `POST {conversation}/api/conv/addUnreadMark` or `/api/conv/removeUnreadMark` `?zpw_ver=694&zpw_type=30` (default host: `https://tt-convers-wpa.chat.zalo.me`), form body `params=<enc>`
- **Encoding:** session AES of an **outer** object `{ "param": "<JSON string of inner>" }`. Commands `12013` / `12014` `[I]`.
- **Params (inner, plaintext):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `convsGroup` | array of string | yes | group ids (no `g` prefix) |
  | `convsUser` | array of string | yes | peer uids |
  | `imei` | string | add only | `<imei>` |
  | `convsGroupData`, `convsUserData` | array | remove only | sent as `[]` |
- **Response:** standard envelope. `[U]` data shape.
- **Errors:** see [errors.md](errors.md).
- **Notes:** `[I]`.

### Mute snapshot and mute changes
Mute affects badges. `getmute` (cmd `11280`) returns `{chatEntries: [...], groupChatEntries: [...]}` (possibly a JSON string, possibly wrapped in `data`); entry `{id, muteMode, duration, startTime | systemTime}`. `setmute` (cmd `11288`) changes it. Full request templates: [../api/conversations.md](../api/conversations.md). Interpretation `[I]`:
- Both arrays must be present, else ignore the snapshot. A complete snapshot unmutes conversations that are absent.
- `muteMode == 1` → muted; `muteMode == 0` → not muted; other values → leave unchanged. Missing `muteMode`: muted iff an entry exists with non-zero `duration`.
- `duration == -1` (any negative) → indefinite; `duration >= 0` → until `startTime + duration` (unix s); expired → not muted.
- Live changes arrive as `act_type: "mute"` control events ([../realtime/events.md](../realtime/events.md) §3).

## Open questions

- `[U]` Seen / delivered receipt endpoints (send and receive).
- `[U]` Whether the server's unread count per conversation is available directly (instead of recomputing from cursors).
- `[U]` Meaning of `type`/`sct` sub-chat cursors.
- `[U]` Whether `lastMsgId` cursors are server IDs or client IDs in all sources.
- Note: in the previous client the unread state machine and manual-mark fetch were implemented and unit-tested but largely not wired into the UI; treat the algorithm as a design, not field-proven.
