# Message identity

Rules for deciding whether two message records (a local pending send, a send acknowledgement, a WebSocket push, a long-poll entry, a history page, a cached row) are the same message, and how to merge them. The guiding principle: **only explicit IDs establish identity; text, timestamps and display names never do.**

Related docs: [message-model.md](message-model.md) (fields), [../api/messages-send.md](../api/messages-send.md) (`clientId`), [../realtime/events.md](../realtime/events.md), [../api/history-sync.md](../api/history-sync.md), [reactions.md](reactions.md).

Evidence: these rules come from the previous client's design and its tests with synthetic fixtures (equal text/time collisions, server-first arrival, replays, 5 000 + 5 000 message merges). They are `[I]` client rules, not live protocol captures, except where marked.

---

## 1. ID namespaces

| Namespace | Source | Notes |
|---|---|---|
| Server ID | wire `msgId` (also `globalMsgId`, `lastMsgId`, `id`) | Assigned by the server; returned as `data.msgId` in a send response `[I]` |
| Client ID | wire `cliMsgId` (also `clientId`, `client_message_id`); request field `clientId` on send | Chosen by the sender and echoed back `[I]` |
| Local pending ID | e.g. `out_<clientId>` | Exists only on this device before the server answers |
| Stored canonical ID | whatever the local store keyed the row by | Legacy: the previous client picked `msgId`, else `cliMsgId`, else `globalMsgId`… |

Validity `[I]`: an ID is usable only if it is a non-empty string other than `"0"` (after trimming), or a positive integral JSON number (converted to its exact decimal text — never via float). Numeric `0`, negative or fractional numbers, `""` and `"0"` are **not** identities. A JSON number and a string with the same decimal text are the same ID; `"00123"` and `"123"` are **different** strings and must not be coerced for matching (exception: read-cursor ordering, [unread.md](unread.md)).

Sender `"0"` stays valid as the "sent by me" sentinel ([message-model.md](message-model.md)).

---

## 2. Client ID allocation (sender side)

- `[I]` Allocate the `clientId` **once per logical send**, before the first request, and reuse the same value across every retry and fallback path of that send (e.g. link → plain text fallback, quote → plain text fallback, attachment upload → send). Return it with the result so the local row keeps it.
- `[I]` Shape: millisecond Unix time, made strictly monotonic within the process: `id = max(now_ms, last_id + 1)`. Two sends in the same millisecond must not share an ID.
- `[U]` Whether the server rejects or deduplicates a repeated `clientId`.

---

## 3. Matching algorithm

Build an index from each existing row to its keys:

```
keys(row):
    if row has a valid server ID: add ("server", id)
    elif row is a local pending row: add ("local", local_id)
    if row is not local pending and its stored ID is valid: add ("stored", stored_id)   # legacy alias
    if row has a consistent client ID: add ("client", client_id)
```

- "Server ID" of a row: its wire server ID if the stored ID is empty, a local pending ID, or equal to its wire client ID; else its stored ID.
- "Consistent client ID": the stored client ID, unless the wire client ID differs from it (conflict → no client key).

To find the match for an incoming record, try its keys in order; for each key with existing rows:

1. If **more than one** row has the key → **no match** (ambiguous; never pick the first).
2. Reject if direction differs (incoming vs outgoing).
3. Reject if both rows have a known wire sender and they differ.
4. `server` / `local` key → match.
5. `stored` key → match unless both rows carry explicit wire server IDs that differ (a legacy alias never overrides a server-ID conflict).
6. `client` key → match only when it bridges a local pending row and a server row of an outgoing message (either arrival order), or when neither side has a server ID yet and one side has an empty stored ID (client-only replay). **Two distinct pending rows or two distinct server rows never merge by client ID.**

Never match on: text, timestamp, display name, attachment URL, or position.

Complexity: index lookup and update are expected O(n + m) followed by a timestamp sort; scale-tested with 5 000 + 5 000 rows `[I]`.

---

## 4. Merging a match

When a match is found `[I]`:

- Remove the old row's keys from the index, merge, then re-insert the merged row's keys (IDs can change).
- Promote a local pending ID to the server ID once known.
- Keep existing enrichment (sticker, link preview, quote, attachment metadata) unless the existing value is missing.
- Merge wire metadata key-by-key (incoming keys overwrite, nothing deleted), merge delivery state by rank ([message-model.md](message-model.md) §5), merge reactions per [reactions.md](reactions.md).
- Timestamp: take the incoming server timestamp if positive. **Exception:** a local send acknowledgement whose match already has a server ID must not change the timestamp — acknowledgement receipt time is not the message time. (Test: server message A at 1000 and B at 2000; A's ack arriving at 3000 must leave A before B.)
- Recall: if the incoming record marks the message recalled, replace content with the recalled placeholder and clear sticker/link fields.

No match → insert as a new row. After a batch, sort by timestamp.

### Arrival orders that must work

- Send response first, then WebSocket/long-poll echo of the same message.
- Echo first (push precedes the local insert), then the send response.
- Repeated acknowledgements and replayed pushes (idempotent).
- History page overlapping already-live rows.

---

## 5. Recall targets

`[I]` A `chat.undo` record identifies its **target** by the `globalMsgId` / `msgId` / `cliMsgId` inside its content. The record's own `msgId` is the recall event's ID and must not replace the target's identity; apply the recall to the matched target row and do not insert a separate visible row.

---

## 6. Persistence note

`[I]` A store that keeps only one canonical ID per row (as the previous client did) loses the alternative namespaces and forces the legacy `stored` alias rules above. A new store should persist server ID, client ID and the raw wire object separately so that matching can always use exact namespaces.

## Open questions

- `[U]` Whether `msgId` is globally unique or only unique per conversation; whether `globalMsgId` ever differs from `msgId` for the same message.
- `[U]` Server behaviour on duplicate `clientId`.
- `[U]` Whether WebSocket pushes of one's own sends always echo `cliMsgId`.
