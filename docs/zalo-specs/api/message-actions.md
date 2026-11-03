# Message actions

Operations on an already-sent message: reactions, delete-for-me and recall (undo for everyone), for 1-1 and group conversations. Sending is covered in [messages-send.md](messages-send.md).

Evidence note: paths, field names and Zalo cmd IDs come from the Zalo PC web bundle (`compact-app-pc.js`: `sendReactionMessage`, `deleteOneOneMessageV2` / `deleteGroupMessageV2`, `undoMessage`) and were implemented in the previous client; no request/response capture was recorded. Everything here is `[I]` unless marked otherwise.

## Common mechanics

Same as [messages-send.md](messages-send.md) §1: `POST`, form body `params=<url-encoded base64 AES(zpw_enk, JSON)>`, query `zpw_ver=<ver>&zpw_type=30`, no signkey, `zpw_sek` cookie; response may be an encrypted envelope (see [../security/payload-encryption.md](../security/payload-encryption.md)). Success is `error_code == 0`; see [../client-rules/errors.md](../client-rules/errors.md). [I]

### Message references

Actions identify the target message by two IDs:

| Name on wire | Meaning |
|---|---|
| `globalMsgId` / `gMsgID` / `msgId` | Server-assigned message ID (`msgId` on the incoming message) |
| `cliMsgId` / `cMsgID` / `cliMsgIdUndo` | The sender's client message ID (`clientId` at send time, echoed as `cliMsgId`) |

See [message-identity.md](../client-rules/message-identity.md) and [../client-rules/message-model.md](../client-rules/message-model.md).

- [I] **Local-only rule:** a message that has no server ID yet (still pending / optimistic) is never sent to these endpoints. Delete and recall are applied locally only; a reaction is sent with an empty `gMsgID`.
- [I] **Contradiction to verify:** the previous client filled every client-ID slot (`cMsgID`, `cliMsgId`, `cliMsgIdUndo`) with the target message's **timestamp in ms** (`ts`), falling back to "now", rather than the real `cliMsgId` from the wire. For own messages the timestamp often equals the `clientId`; for others' messages it generally does not. A re-implementation should send the real `cliMsgId` and verify against a capture `[U]`.

## Reactions

### Icon ↔ code mapping  [I]

| Emoji | `rIcon` | `rType` |
|---|---|---|
| 👍 | `/-strong` | `3` |
| ❤️ | `/-heart` | `5` |
| 😆 | `:>` | `0` |
| 😮 | `:o` | `32` |
| 😢 | `:-((` | `2` |
| 😠 | `:-h` | `20` |

- Source: Zalo PC bundle (`compact-app-pc.js`) reaction table. [I]
- Some data had `:-((` with a trailing space (`":-(( "`); treat it as `:-((`. [I]
- Unknown icons were mapped to ❤️ (`/-heart`, `5`) on send, and to a neutral placeholder on display. [I]
- Incoming reaction events with `rType == -1` were treated as "this reactor cleared their reaction". [I] The client never sent `-1`; how to remove one's own reaction on the wire is `[U]`.
- Zalo supports more reaction icons than these six `[U]`.

### Send reaction (1-1)
- **Request:** `POST {reaction}/api/message/reaction?zpw_ver=<ver>&zpw_type=30`  (default host: `https://reaction.api.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`); no signkey
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `react_list` | array | yes | One element per reaction, see below |
  | `toid` | string | yes | Peer user ID |
- **`react_list[i]`:**
  | Field | Type | Meaning |
  |---|---|---|
  | `message` | string (JSON) | JSON-encoded string of the inner object below |
  | `clientId` | int | Current time ms (fresh per reaction) |
- **Inner `message` object (serialised to a string):**
  ```json
  {"rMsg":[{"gMsgID":"<server msg id>","cMsgID":<client msg id>,"msgType":1}],
   "rIcon":"/-heart","rType":5,"source":6}
  ```
  - `gMsgID`: target server ID as a **string**; `""` if not yet known.
  - `cMsgID`: target client message ID as a **number** (see the contradiction above).
  - `msgType`: `5` if the target is a sticker, else `1`. Values for photo/file/link targets `[U]`.
  - `source`: `6`. Meaning `[U]`.
- **Response:** envelope; content not used beyond `error_code`.
- **Errors:** non-zero → reaction not applied.
- **Notes:** 1-1 body has **no `imei`**. Zalo cmd `12410`/`12411` per bundle reference; which is 1-1 vs group is `[U]`. [I]

### Send reaction (group)
- **Request:** `POST {reaction}/api/group/reaction?zpw_ver=<ver>&zpw_type=30`  (default host: `https://reaction.api.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`); no signkey
- **Params:** `react_list` (same as 1-1), `grid` (bare group ID), `imei`. [I]
- **Response / Errors:** as 1-1.
- **Notes:** the `reaction` host is in the service map (key `reaction`) [O]; the previous client used the default host directly rather than the service-map value [I] — a re-implementation should read it from the map ([../auth/service-discovery.md](../auth/service-discovery.md)).

Reaction events from others arrive over the realtime channel; see `realtime/` docs and [../client-rules/message-model.md](../client-rules/message-model.md).

## Delete for me

Removes the message from the caller's own view only; other participants still see it.

### Delete for me (1-1)
- **Request:** `POST {chat}/api/message/delete?zpw_ver=<ver>&zpw_type=30`  (default host: `https://tt-chat3-wpa.api.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`); no signkey
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `toid` | string | yes | Peer user ID |
  | `cliMsgId` | int | yes | Fresh client ID for this request (current ms) |
  | `msgs` | array | yes | Messages to delete, see below |
  | `onlyMe` | int | yes | `1` (delete for me) |
  | `imei` | string | yes | Login IMEI |
- **`msgs[i]`:**
  | Field | Type | Meaning |
  |---|---|---|
  | `cliMsgId` | string | Target's client message ID (string here) |
  | `globalMsgId` | string | Target's server ID |
  | `ownerId` | string | UID of the target's sender: own UID if outgoing, else the peer ID |
  | `destId` | string | The conversation ID (peer ID) |
- **Response:** envelope; only `error_code` used.
- **Notes:** Zalo cmd `11779` (`deleteOneOneMessageV2`) per bundle reference. Same `chat` host rules as [messages-send.md](messages-send.md) §1.1. After success the client also deletes the row from its local store. [I]

### Delete for me (group)
- **Request:** `POST {group}/api/group/deletemsg?zpw_ver=<ver>&zpw_type=30`  (default host: `https://tt-group-wpa.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`); no signkey
- **Params:** `grid` (bare group ID), `cliMsgId` (fresh, current ms), `msgs` (as above), `onlyMe: 1`. **No `imei`.** [I]
  - `ownerId`: own UID if outgoing, else the sender's UID.
  - `destId`: the group ID.
- **Notes:** Zalo cmd `11834` (`deleteGroupMessageV2`). Two likely defects in the previous client, both `[U]` until verified: `destId` was the local thread ID (possibly with a `g` prefix) rather than the bare group ID, and `ownerId` for others' messages was the sender's *display name* field, which may not be a UID. Use the bare group ID and the numeric sender UID. `onlyMe: 0` presumably deletes for everyone (group admin delete) `[U]`.

## Recall (undo for everyone)

Replaces the message for all participants with a "message recalled" placeholder. The UI offered this only for the user's **own outgoing** messages. [I]

### Recall (1-1)
- **Request:** `POST {chat}/api/message/undo?zpw_ver=<ver>&zpw_type=30`  (default host: `https://tt-chat3-wpa.api.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`); no signkey
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `msgId` | string | yes | Target server ID |
  | `toid` | string | yes | Peer user ID |
  | `clientId` | int | yes | Fresh client ID for this request (current ms) |
  | `cliMsgIdUndo` | string | yes | Target's client message ID |
- **Response:** envelope; only `error_code` used.
- **Notes:** Zalo cmd `11281` (`undoMessage`). **No `imei`** in 1-1. [I]

### Recall (group)
- **Request:** `POST {group}/api/group/undomsg?zpw_ver=<ver>&zpw_type=30`  (default host: `https://tt-group-wpa.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`); no signkey
- **Params:** `msgId`, `grid` (bare group ID), `clientId` (fresh), `visibility: 0`, `cliMsgIdUndo`, `imei`. [I]
- **Notes:** Zalo cmd `11192`. [I]

### Recall rules  [I]

- The previous client applied the recall **optimistically**: it replaced the local text with the recalled placeholder and dropped sticker/link/thumbnail before the request returned, and did not roll back on server failure (it only showed an error status). A re-implementation should prefer rolling back or marking "recall failed".
- No client-side time limit was enforced. Zalo's server-side recall window is `[U]`.
- Recall events from others (and the server echo of one's own) arrive as messages with `msgType` `chat.undo` (numeric `31`), whose content names the target via `globalMsgId` / `msgId` / `cliMsgId`. See [../client-rules/message-model.md](../client-rules/message-model.md).

## Undo of other actions

No "undo reaction" or "undo delete" request was implemented. [I]

## Open questions

- [U] Whether `cMsgID` / `cliMsgId` / `cliMsgIdUndo` must be the real wire `cliMsgId` (likely) — the previous client sent the message timestamp.
- [U] Recall time limit enforced by the server, and its error code when exceeded.
- [U] How to remove one's own reaction (is `rType: -1` sent, or a separate endpoint?); full list of reaction icons/types; meaning of `source: 6`; `msgType` values in `rMsg` for photos/files/links.
- [U] Which of cmd `12410` / `12411` is 1-1 vs group.
- [U] Group delete: correct `destId` form and `ownerId` source; whether `onlyMe: 0` enables admin delete-for-everyone.
- [U] Whether the omitted `imei` (1-1 reaction, 1-1 undo, group delete) is truly optional.
- [U] Response `data` shapes for all three actions (only `error_code` was read).
