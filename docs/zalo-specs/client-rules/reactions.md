# Reactions

Wire format of message reactions (summary snapshots and incremental events), the reaction type codes, how to send one, and how a client aggregates them without losing counts or mangling message IDs.

Related docs: [message-model.md](message-model.md), [message-identity.md](message-identity.md), [../realtime/events.md](../realtime/events.md), [../api/message-actions.md](../api/message-actions.md).

---

## 1. Reaction type codes

`rType` (number) and `rIcon` (legacy text code) `[I]` (sender-side mapping in the previous client; `[U]` whether the official client has more types):

| `rType` | `rIcon` | Emoji |
|---|---|---|
| 3 | `/-strong` | 👍 |
| 5 | `/-heart` | ❤️ |
| 0 | `:>` | 😆 |
| 32 | `:o` | 😮 |
| 2 | `:-((` | 😢 |
| 20 | `:-h` | 😠 |
| -1 | — | **Clear**: remove all of this sender's reactions on the target |

Rules `[I]`:
- An unknown `rType` (e.g. `999`) is **not** a heart; keep it as unknown with a neutral glyph.
- `rType < -1` is invalid; drop the event.
- Note: the previous client's sender mapped any unrecognised emoji to heart (`/-heart`, 5). Do not copy that; refuse to send unknown reactions instead.

---

## 2. Summary snapshot (attached to a message)

`[I]` Found at `reacts.reactions` (or `reacts`, or `reactions`) of a message:

```
{ "reacts": { "reactions": { "<uid>": { "<rType>": <count>, ... }, ... } } }
```

- Per-user, per-type **counts** (one user may react the same type several times). Total for a type = sum over users; this is **not** the number of distinct users.
- A count may be a number, a numeric string, or the string `"9k+"` (lower bound: treat as 9001 and flag "at least").
- Ignore non-positive counts and negative type keys.
- Legacy shape: an array of icons (`["❤️", "👍"]`) or `[{rIcon, count}]`. Legacy icon lists represent unique icons, not multiplicity.
- An **absent or null** field is **not** an authoritative "no reactions"; keep existing state. Only an explicit snapshot replaces.

---

## 3. Reaction events

`[I]` An event is a message-shaped object with `msgType: "chat.reaction"`:

| Key | Meaning |
|---|---|
| `uidFrom` | Reactor uid (required) |
| `idTo` | Conversation: peer uid, group id, or `"0"` (then the conversation is `uidFrom`) |
| `msgKey` | Conversation id when the content is wrapped in `params` |
| `msgId`, `cliMsgId` | The **event's** own server / client IDs (used for dedup), not the target's |
| `ts` | Event time (ms or s) |
| `content` | Object or JSON string: `{rType, rIcon, rMsg: [{gMsgID, cMsgID, msgType}], source}`; or `{params: "<that JSON as string>"}` |

- `rMsg` may list **several targets**; produce one event per target.
- Target identity: `gMsgID` (server ID) and/or `cMsgID` (client ID). `"0"` means absent. Drop a target with neither.
- Containers seen `[I]`: `reacts: [...]` (1-1), `reactGroups: [...]` (groups: `idTo` is the group id → conversation `"g" + idTo`), `reactions: [...]`, or `messages: [...]` filtered to `msgType == "chat.reaction"`.
- `[U]` Which WebSocket command carries live reaction events; socket dispatch must identify the reaction channel before parsing (the parser is shape-based).
- Note: in the previous client the event parser existed and was tested but was not wired to the transports.

### Lossless IDs — `[I]`

`gMsgID`, `cMsgID`, `msgId` may be JSON **numbers above 2⁶⁴−1** (fixture: `18446744073709551613`). Parse them with an arbitrary-precision JSON number reader and keep the exact decimal text. Never round-trip through float or a fixed 64-bit signed integer.

---

## 4. Sending a reaction

### Send reaction (1-1 / group)
- **Request:** `POST {reaction}/api/message/reaction?zpw_ver=694&zpw_type=30` (1-1) or `POST {reaction}/api/group/reaction?…` (group) (default host: `https://reaction.api.zaloapp.com`); form body `params=<enc>`
- **Encoding:** session AES (`zpw_enk`); commands `12410` / `12411` `[I]`.
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `react_list` | array of `{message: <JSON string>, clientId: <ms>}` | yes | one reaction |
  | `react_list[].message` | JSON string of `{rMsg:[{gMsgID, cMsgID, msgType}], rIcon, rType, source: 6}` | yes | `gMsgID` = target server ID (`""` if only local), `cMsgID` = target client ID, `msgType` = 1 text / 5 sticker `[U]` |
  | `toid` | string | 1-1 | peer uid |
  | `grid` | string | group | group id without prefix |
  | `imei` | string | group | `<imei>` |
- **Response (`data` after decryption):** `[U]`.
- **Errors:** see [errors.md](errors.md).
- **Notes:** `[I]` The previous client sent the target's **timestamp** as `cMsgID` when it had one; send the real `cliMsgId` instead and record this as an open question. Clear = `rType -1` `[U]` on send.

---

## 5. Aggregation algorithm

Goal: correct per-type counts under replay, out-of-order delivery, and mixed snapshot/event sources. `[I]`

Per message keep:
- `summaries`: list of `{rType, emoji, count, reactor_ids, reactor_counts{uid: n}, count_is_lower_bound}`.
- Per-reactor ledger `{baseline{rType: n}, events[], covered_events[], last (ts, event_id, client_event_id)}`.
- Snapshot provenance `{source, captured_at_ms}`.

Event ordering: by normalized ms `ts`, then client event ID, then server event ID (each by length then lexicographic), then a clear (`-1`) sorts after a non-clear on a full tie (clear wins).

Event equality (dedup): same non-empty, non-`"0"` server event ID, or same client event ID. If either side has any ID but they don't match → different. Only when **both** sides have no IDs: same positive ts (normalized) and same `rType`.

Apply one event:

```
clear (-1): for every summary, subtract this reactor's count, remove reactor; drop empty summaries
else:       summary[rType].count += 1; add reactor; reactor_counts[uid] += 1
```

Recording an event for a reactor:
1. On first use, initialise the ledger baseline from the current snapshot's per-reactor counts.
2. If the event equals any recorded or covered event → ignore (replay).
3. Insert, keep `events` sorted, then **rebuild** that reactor: clear them, re-add baseline, re-apply events in order (skip historical events already covered by an authoritative snapshot).

Snapshot acceptance — a snapshot may replace summaries only if:
- it is authoritative and network-fresh (a cached snapshot never overrides tracked live events), and
- its `captured_at_ms` is newer than the current snapshot's, and
- it is not older than the newest live (non-historical) event already applied.

On accepting, move each reactor's events to `covered_events` and reset baselines from the snapshot. For non-authoritative snapshots, replace only reactors with no ledger; never rewind tracked reactors.

Coverage: keep events whose target message could not be found as "unresolved"; while any remain, do not claim the reaction view is complete.

Display order: by total count desc, then `rType`, then emoji.

## Open questions

- `[U]` Full `rType` vocabulary; whether `rIcon` is still required on send.
- `[U]` The live transport/command for reaction events.
- `[U]` Response shape of the send endpoint; whether `cMsgID` must be the real `cliMsgId`.
- `[U]` Meaning of `source: 6` and of `msgType` inside `rMsg`.
