# Message model

The fields of a Zalo wire message as delivered by send acknowledgements, history, previews, long-polling and the WebSocket, the message-type vocabulary, content shapes per type, and the conservative rules a client should follow when interpreting them.

Related docs: [message-identity.md](message-identity.md) (matching and merging), [unread.md](unread.md), [reactions.md](reactions.md), [../realtime/events.md](../realtime/events.md), [../api/messages-send.md](../api/messages-send.md), [../api/message-actions.md](../api/message-actions.md), [../api/history-sync.md](../api/history-sync.md), [../media/downloads.md](../media/downloads.md), [../media/stickers.md](../media/stickers.md).

General rule `[I]`: **keep the original wire object losslessly** (all keys and values, including unknown ones) next to whatever the client derives from it. Derived fields can be recomputed when the rules here improve; a discarded wire field cannot be recovered.

---

## 1. Wire fields

Field names vary between endpoints; a parser must accept the aliases listed (first match wins, in the order shown). Tags apply to the field's existence and meaning.

| Concept | Wire keys (priority order) | Type on the wire | Meaning | Tag |
|---|---|---|---|---|
| Server message ID | `msgId`, then `globalMsgId`, `lastMsgId`, `id` | string **or** number | Server-assigned ID; unique per message. `globalMsgId` appears in cloud/group history and quotes | `[I]` |
| Client message ID | `cliMsgId`, then `clientId`, `client_message_id` | string or number | Client-chosen ID echoed back by the server; on send the request field is `clientId` | `[I]` |
| Sender | `uidFrom`, `fromUid`, `fromId`, `senderId`, `sender` | string or number | Sender uid. **`"0"` = the current account** (outgoing) | `[O]` (`"0"` sentinel), `[I]` aliases |
| Destination | `idTo`, `toId`, `toUid`, `grid`, `groupId` | string or number | Peer uid (1-1) or group id. `"0"` on incoming 1-1 = "to me" | `[O]` routing ([../realtime/events.md](../realtime/events.md) §2.1) |
| Group flag | `isGroup` | bool or 0/1 | Message belongs to a group | `[I]` |
| Type | `msgType`, then `messageType`, `type` | string (`"chat.photo"`) **or** number | Message type, see §2 | `[I]` |
| Timestamp | `ts`, then `sendTime`, `serverTime`, `time` | number or numeric string | Unix time, usually ms; values `< 10¹⁰` are seconds | `[I]` |
| Display name | `dName`, `displayName`, `fromName`, `senderName` | string | Sender display name at send time. **Never an identity** | `[I]` |
| Content | `content`, then `message.content`, `message`, `msg` | string, JSON-string, or object | Body, see §3 | `[I]` |
| Quote | `quote` | object | Reply metadata, see §4 | `[I]` |
| Mentions | `mentions` (live), `meta.mentionsList` (archive) | array of `{uid, …}` | Mentioned users; `uid == "-1"` = mention all | `[O]` ([unread.md](unread.md)) |
| Reactions | `reacts` / `reactions` | object or array | Reaction summary, see [reactions.md](reactions.md) | `[I]` |
| Unread policy | `paramsExt.countUnread` | number | Whether the message counts toward unread, see [unread.md](unread.md) | `[O]` |
| Other kept fields | `cmd`, `st`, `at`, `ttl`, `paramsExt`, `meta`, `attachsList` | various | `[U]` semantics of `cmd`, `st`, `at` | `[U]` |

Identity value rules (details in [message-identity.md](message-identity.md)):
- IDs are 64-bit-plus integers that can exceed 2⁵³ and even 2⁶³. **Never parse them into floating point.** Keep the textual form; a JSON number is converted to its exact decimal text. `[O]` (the bundle uses string IDs; the client tests use values above 2⁶³)
- Empty string, `"0"`, numeric `0`, negative or fractional numbers are **not** identities. `[I]`
- `[I]` The previous client assumed `cliMsgId` is a client-side millisecond timestamp and used it as a timestamp fallback; it also sent a message's `ts` as `cMsgID`/`cliMsgIdUndo` when reacting/recalling. Treat both as `[U]` assumptions: keep `cliMsgId` as an opaque ID.

### `ttl`

`[I]` Send bodies (text, quote, link) carry `ttl: 0`. `[U]` Non-zero `ttl` presumably marks a disappearing message (lifetime in ms or s); not observed on received messages.

---

## 2. Message types

### 2.1 String `msgType` — primary key

String types are the reliable discriminator. A client must classify on the string whenever one is present. Names marked `[O]` come from the bundle's decoder table that maps archive codes to wire names; the rest were accepted by the previous parser `[I]`.

| String `msgType` | Kind | Tag |
|---|---|---|
| `webchat` | Text (the live text type) | `[O]` |
| `chat.text`, `text` | Text | `[I]` |
| `chat.doodle` | Doodle (drawing; rendered as photo) | `[O]` |
| `chat.photo` | Photo | `[O]` |
| `chat.voice` | Voice note | `[O]` |
| `chat.sticker` | Sticker | `[O]` |
| `chat.recommended` | Contact card / recommendation ("danh thiếp"); also used for shared links by the previous parser | `[O]` name, `[U]` link usage |
| `chat.list.action` | OA action list | `[O]` |
| `chat.location.new`, `chat.location` | Location | `[O]` / `[I]` |
| `chat.video.msg`, `chat.video` | Video | `[O]` / `[I]` |
| `chat.video.live.msg` | Live video | `[O]` |
| `share.file`, `chat.file` | File | `[O]` / `[I]` |
| `chat.gif` | GIF | `[O]` |
| `chat.webcontent`, `chat.webcontent.v2` | Web content card | `[O]` |
| `chat.delete` | Delete event (**not** a recall) | `[O]` |
| `chat.undo` | Recall ("thu hồi") event targeting another message | `[I]` |
| `chat.reaction` | Reaction event ([reactions.md](reactions.md)) | `[I]` |
| `chat.contact`, `chat.system`, `system` | Contact / system message | `[I]` |
| anything else | Unknown: keep the raw object, render a neutral placeholder | — |

Strings should be compared case-insensitively `[I]`.

### 2.2 Numeric `msgType` — conflicting mappings, all `[U]`

Some sources (archive/phone-sync history `history_source.msgType`, and occasionally live payloads) carry a **number** instead of a string. The previous code contained **three mutually inconsistent mappings**, plus a fourth set of internal UI codes. No capture or fixture proves any live numeric code, so every row is `[U]`. Do not classify by number when a string type or a content shape is available.

| Code | (A) live/poll parser fallback | (B) history media hydration | (C) archive enum + decoder (bundle `Ai`/`ji` tables) | (D) bundle internal UI code (not a wire code) |
|---|---|---|---|---|
| 0 | — | — | Text → `webchat` | — |
| 1 | Text | — | — | Text |
| 2 | Photo | Photo | Doodle → `chat.doodle` | Photo |
| 3 | Voice | Photo | Photo → `chat.photo` | Voice |
| 4 | Video | Photo | Photo_V2 → `chat.photo` | Sticker |
| 5 | Sticker | — | — | — |
| 6 | — | — | Voice → `chat.voice` | Contact |
| 7 | — | — | — | GIF |
| 10 | — | — | Sticker → `chat.sticker` | — |
| 11 | — | — | — | Photo |
| 12 | — | — | Contact → `chat.recommended` | — |
| 15 | — | — | OA → `chat.list.action` | — |
| 17 | — | — | — | Location |
| 18 | — | — | Location → `chat.location.new` | Video |
| 19 | **File** | **Video** | **Video** → `chat.video.msg` | File |
| 20 | — | — | ChatInfo (no decoder) | Recall |
| 21 | — | — | ECard (no decoder) | — |
| 22 | — | — | File → `share.file` | — |
| 23 | — | — | Gift/GIF → `chat.gif` | — |
| 24 | **Location** | — | **Webcontent** → `chat.webcontent` | — |
| 25 | — | — | VideoLive → `chat.video.live.msg` | — |
| 26 | — | — | Poll (no decoder) | — |
| 31 | **Recall (`chat.undo`)** | — | **PhotoV2** → `chat.photo` | — |
| 33 | — | — | ChatDelete → `chat.delete` | — |
| 36 | — | — | Undo (no decoder) | — |
| 53 | — | — | WebcontentV2 → `chat.webcontent.v2` | — |

Provenance and confidence:
- (A) A fallback in the previous live/poll parser. No evidence at all; it conflicts with (C) on 2, 3, 4, 19, 24, 31. `[U]`
- (B) Used only to repair media labels on rows from archive/phone history; agrees with (C) on 3, 4 and 19 but calls 2 a photo (C: doodle, which renders as an image anyway). `[U]`
- (C) Read from the bundle's archive-type enum (`Ai`) and its archive→wire decoder (`ji`) in the shared worker. This is the best-sourced table, but it describes the **archive** (cloud/phone-history) format; no live fixture confirms it. Codes named in the enum but absent from the decoder (20, 21, 26, 36) have no verified wire name. `[U]` for live use; the table itself was read from the bundle.
- (D) The bundle's internal UI enum. It is **not** a protocol code; listed only so nobody mistakes it for one.

Recommendation: treat a numeric type as (C) **only** for archive/phone-history records, and only when the content shape agrees; otherwise classify by content shape (§3) and keep the number raw. Note in particular that archive 33 (`chat.delete`) is **not** a recall. [../media/downloads.md](../media/downloads.md) and [../media/stickers.md](../media/stickers.md) defer to this section.

---

## 3. Content shapes

`content` may be a plain string, a **JSON string** that must be parsed again, or an object. Tags `[I]` unless noted.

| Kind | Shape (keys used) | Notes |
|---|---|---|
| Text | string | The literal `"sendBubbleMessage"` is a placeholder: skip the message. |
| Rich text | `{action: "rtf", title: <text>, params: <JSON string of styles>}`; on send, `textProperties` = `{"styles":[{"start":N,"len":M,"st":"b,i,u,s,f_18,lst_1,lst_2"}],"ver":0}` | `title` is the plain text. See [../api/messages-send.md](../api/messages-send.md). |
| Photo | `{href, thumb or hd, description or caption, params}` | Caption from `description`/`caption`. URL hosts are photo CDNs. |
| File | `{href, title or fileName or name, params: JSON string {fileName, fileExt, fileSize, checksum, src, mediaTitle}}` | Append `.fileExt` to the title if missing; `href` containing `/renewmedia` indicates a file. See [../media/downloads.md](../media/downloads.md). |
| Sticker | `{catId or cateId, stickerId or id, type}` (default `type` 7) | See [../media/stickers.md](../media/stickers.md). |
| Link | `{href, title, description, thumb, params: JSON string {src, …}}` | |
| Video / voice / location / contact / GIF / webcontent | object; `[U]` exact fields | Keep raw. |
| Recall (`chat.undo`) | object or JSON string with `globalMsgId`, `msgId`, `cliMsgId` of the **target** | Apply to the target ([message-identity.md](message-identity.md) §5). |
| Reaction (`chat.reaction`) | `{rType, rIcon, rMsg: [{gMsgID, cMsgID, msgType}], source}` or `{params: <that as JSON string>}` | [reactions.md](reactions.md). |

When `content` is absent, attachment metadata may be in `meta.attachsList[0]` (archive rows) `[I]`.

---

## 4. Quote (reply) and forward metadata

Received quote object `quote` `[I]`:

| Key | Meaning |
|---|---|
| `cliMsgId`, `globalMsgId`, `msgId` | Reference to the quoted message (try in this order; skip empty/`"0"`) |
| `fromD`, `dName`, `ownerId`, `fromUid`, `uidFrom` | Quoted sender (display name or uid) |
| `msg`, `message`, `content` | Quoted text (may be an object → extract text as §3) |

Archive rows carry the same object at `meta.quote` `[I]`.

Resolving a quote to a loaded message `[I]`: match the reference against **every** ID namespace of each loaded message (stored ID, client ID, wire server ID, wire client ID); jump only if **exactly one** message matches. Text and timestamps never disambiguate; numeric coercion is not allowed (`"00123"` ≠ `"0123"`).

Send-side quote fields (`qmsgOwner`, `qmsgId`, `qmsgCliId`, `qmsgType`, `qmsg`, `qmsgTs`) are in [../api/messages-send.md](../api/messages-send.md).

Forward `[I]`: the previous client re-sent text or sticker content as a new message (no forward metadata). `[U]` Wire fields that mark a received message as forwarded.

---

## 5. Derived semantics (conservative rules)

- **Direction** `[I]`: outgoing iff sender is `"0"` or empty, or explicit `isOutgoing: true` / `is_outgoing: 1`. Otherwise incoming. (A sender equal to the own uid should also be outgoing — `[U]` whether servers ever send that.)
- **System messages** never count as unread and are never "mentions" ([unread.md](unread.md)).
- **Delivery state** `[I]`: a local state per outgoing message with a monotone rank; merges keep the higher rank and never regress:

  | Rank | State | Meaning |
  |---|---|---|
  | 0 | unknown | nothing known |
  | 1 | pending | request not yet answered |
  | 2 | delivery-unknown | answer ambiguous (timeout etc.) |
  | 3 | failed | server returned an error |
  | 4 | server-accepted | `error_code 0` with `msgId` |
  | 5 | delivered | peer device received (`[U]` wire event) |
  | 6 | read | peer read (`[U]` wire event) |

  Errors and uncertainty never imply delivery.
- **Timestamps**: normalize to ms (values `< 10¹⁰` are seconds). A local send acknowledgement time is **not** a server timestamp ([message-identity.md](message-identity.md)).
- **Ordering**: sort by server timestamp; ties are not identity evidence.

## Open questions

- `[U]` Every numeric `msgType` in §2.2 for live payloads.
- `[U]` Semantics of `cmd`, `st`, `at`, non-zero `ttl`.
- `[U]` Whether `cliMsgId` is guaranteed to be a send-time millisecond value.
- `[U]` Forward markers; content shapes for video, voice, location, contact, GIF, webcontent, poll.
- `[U]` `chat.delete` vs. `chat.undo` semantics and who may issue each.
- `[U]` Whether `chat.recommended` is used for plain links or only contact cards.
