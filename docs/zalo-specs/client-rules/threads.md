# Thread rules: merging, names, avatars, sidebar, startup sequencing

Client-side rules a Zalo client needs to keep one sidebar entry per real conversation, show stable names and avatars, render previews and timestamps the way the official client does, and load everything in the right order.

Evidence tags: `[O]` observed (live-verified, or read from the shipped official Zalo PC bundle); `[I]` implemented in the previous client without recorded live evidence; `[U]` unknown. Almost everything here is client policy learned from bugs, so most rules are `[I]`; issue numbers (#63 …) refer to the project tracker.

Related: [../api/conversations.md](../api/conversations.md), [../api/contacts.md](../api/contacts.md), [../api/groups.md](../api/groups.md), [message-identity.md](message-identity.md), [message-model.md](message-model.md), [../media/downloads.md](../media/downloads.md).

## 1. Thread model (conceptual)

| Field | Meaning |
|---|---|
| `id` | `<uid>` for 1-1, `<grid>` for groups, `send2me_id` for My Cloud. Empty or `"0"` = invalid, drop. |
| `is_group` | group flag (sticky: once true, stays true) |
| `name` | display name (may be a [generic placeholder](#2-generic-names)) |
| `avatar` | URL, or a client composite `multi:` string for groups ([groups.md](../api/groups.md#group-avatar-selection)) |
| `preview`, `time` | last message text (newlines → spaces) and its formatted time |
| `messages` | recent messages, sorted by timestamp |
| sidebar metadata | `members[]` (id, name, avatar), `member_count` (server `totalMember` only), `last_message_ts`, `last_message_id`, `last_sender`, `last_sender_id`, `last_outgoing`, `pinned`, `muted`, `hidden` |
| unread state | count, marked-unread flag, mute expiry — see [message-model.md](message-model.md) |

Timestamps may arrive in seconds or milliseconds: a positive value `< 10_000_000_000` is seconds (×1000); `<= 0` means unknown. Always normalize before comparing. [I]

## 2. Generic names  [I]

A name is **generic** (a placeholder that any real name should replace) when, after trimming, it is:
- empty, or
- starts with `"Người dùng "` (placeholder for an unknown user: `"Người dùng <uid>"`), or
- starts with `"Nhóm "` (placeholder for an unknown group: `"Nhóm <grid>"`), or
- consists only of ASCII digits (a raw ID).

Caveat: a real group whose name starts with "Nhóm " is also classed as generic, so it will be re-resolved repeatedly and may lose to any other name. [I] A rewrite should track "placeholder" as an explicit flag instead of string matching.

The My Cloud thread is named `"Cloud của tôi (Lưu trữ)"`; rules below that mention My Cloud detect it by the name containing `"Cloud của tôi"`. [I] (A rewrite should detect it by `id == send2me_id`.)

## 3. Avatar selection and replacement  [I]

### 3.1 Helpers

- **File key:** strip the query (`?…`), take the last path segment; if it is ≥ 12 chars and ends with `.jpg`, `.png` or `.webp`, its lowercase form is the stable avatar identity. Zalo CDN URLs for the same picture differ in size subdomain (`s120-…` vs `s160-…`) and in rotating `?key=…&time=…` tokens, but keep the same file name.
- **URL time:** the largest `time=<digits>` query value found in the string (for `multi:` strings, across all segments); `0` if none. A larger value means a URL issued by a newer session.
- **Real URL count:** for a plain avatar, `1` if it starts with `http://`/`https://` and does not end with `/default`, else `0`; for `multi:` strings, the number of slots holding such a URL.
- **Signature:** a plain avatar → its file key (or URL without query); a `multi:` string → the same string with each slot's URL replaced by its file key.

### 3.2 Should `incoming` replace `current`?

```
should_replace(current, incoming):
  if incoming empty: return false
  if current empty:  return true
  if realCount(incoming) < realCount(current): return false      # never downgrade
  if realCount(incoming) > realCount(current): return true       # strictly richer wins
  if both are multi: and realCount(current) > 0: return false    # keep a working composite
  if signature(current) == signature(incoming): return false     # same picture, different CDN token
  return urlTime(incoming) > urlTime(current)                    # newer session's URL wins
```

Purpose: avoid flicker between `s120`/`s160` variants and token rotations, and never replace a composite group avatar that already shows member photos with a poorer one.

### 3.3 Where avatars come from
- 1-1: `getfriends` avatar, else the profile batch call when the thread has no avatar.
- Group: `getmg-v2` `fullAvt`/`avt` unless it ends in `/default`, else a composite built from members; offline fallback from known contacts' avatars. See [../api/groups.md](../api/groups.md#group-avatar-selection).
- All updates from the network go through `should_replace`.

## 4. Same-contact detection and deduplication

### 4.1 Do two threads represent the same conversation?  [I] (#63)

```
same(a, b):
  if a.id non-empty and a.id == b.id: return true
  if a.is_group != b.is_group:        return false
  if not group and both names contain "Cloud của tôi": return true     # singleton My Cloud
  ka, kb = fileKey(a.avatar), fileKey(b.avatar)
  if ka and kb:
     if ka == kb and "default" not in ka:
        return generic(a.name) or generic(b.name) or trim(a.name) == trim(b.name)
     return false
  return false
```

- **Never** merge two different non-empty IDs because the display names match: many contacts and groups share names (#63).
- The avatar-file-key rule exists because, per the previous client's notes, a QR re-login could produce a different session-scoped user ID for the same contact; the old cached thread and the new one are recognised by the same uploaded avatar file. [I] Whether Zalo really re-issues user IDs per session is [U].

### 4.2 Merging thread `incoming` into `target`  [I]

```
merge(target, incoming, incomingIdIsAuthoritative):
  if incomingIdIsAuthoritative and incoming.id not in ("", "0"): target.id = incoming.id
  if incoming.name non-empty and (not generic(incoming.name) or generic(target.name)):
       target.name = incoming.name                       # a real name never loses to a placeholder
  if should_replace(target.avatar, incoming.avatar): target.avatar = incoming.avatar
  target.is_group = target.is_group or incoming.is_group
  for msg in incoming.messages:
     skip if msg.text == "sendBubbleMessage"            # internal placeholder, never shown
     find existing message by identity (message-identity.md)
     if found: merge fields (below) else append
  drop "sendBubbleMessage" rows; sort messages by ts
  if messages non-empty and newest.text non-empty: preview = newest.text (newlines→spaces), time = newest.time
  elif target.preview empty and incoming.preview non-empty: take incoming preview/time
  elif target.time empty: take incoming.time
```

Per-message field merge when an identity match is found:
- a local pending ID (`out_…`) is replaced by the server ID;
- a server timestamp (`ts > 0`) that differs replaces the local one (and its formatted time);
- a recalled message (text `"[Tin nhắn đã thu hồi]"`) clears sticker and link/attachment data;
- sticker, reply-quote, and link/attachment fields are filled only when missing locally (link fields also replace the text if the incoming text is non-empty);
- reactions follow the reaction-merge rules in [message-model.md](message-model.md); pinned/starred flags are OR-ed.

A send acknowledgement merged into a message that already has a server ID must not overwrite its server timestamp: the ack's local receipt time is cleared before merging. [I]

### 4.3 Deduplicating a list  [I]

```
dedup(threads, authoritativeIds = optional set):
  out = []
  for t in threads (in order):
     drop t if t.id in ("", "0")
     remove "sendBubbleMessage" messages; if preview empty/"sendBubbleMessage", recompute from newest
     e = first in out with e.id == t.id, or (unless BOTH ids are authoritative) same(e, t)
     if e:
        if authoritativeIds given: incomingWins = t.id ∈ auth and e.id ∉ auth
        else: incomingWins = urlTime(t.avatar) > urlTime(e.avatar), or true if equal   # later cache entry = newer session
        merge(e, t, incomingWins)
     else out.append(t)
  # My Cloud pollution fix
  cloudIds = server message IDs in My Cloud threads (exclude "", "out_…", "local_…")
  for every other thread: remove messages whose id ∈ cloudIds; recompute preview/time (or clear)
```

- `authoritativeIds` = IDs present in the current live data (existing UI list ∪ incoming update). Two threads that are both live are never merged by the heuristic, only by equal ID.
- The My Cloud cleanup exists because self-messages were sometimes mis-routed into a peer's thread; a message that belongs to My Cloud must not also appear elsewhere. [I]

### 4.4 Applying an update batch to the UI list  [I]

For each incoming thread: (1) exact ID match → `merge(existing, incoming, true)`; (2) else a `same()` match among existing threads whose ID is **not** in this batch → merge with incoming ID authoritative; (3) else append (if ID valid). Then `dedup(list, authoritativeIds)`. Restore the user's selection by exact ID first, falling back to `same()` only if the ID vanished (#63).

## 5. Display-name resolution  [I]

Run after every list build and for threads discovered by polling/websocket:

1. **1-1 threads** (not My Cloud) whose name starts with `"Người dùng "`, is empty, or that have no avatar → batch profile lookup (≤ 50 UIDs per call, [../api/contacts.md](../api/contacts.md#get-user-profiles-names--avatars-for-arbitrary-uids)). Set the name (from `zaloName`, `displayName`, `name`, `display_name`) only if the current one is a placeholder or empty; set the avatar whenever the profile has one.
2. **Group threads** with a generic name, an empty preview, or no avatar → `getmg-v2` (≤ 50 grids per call, version `0`). Set name and real avatar; else collect member UIDs for a composite avatar; set the member-count preview placeholder if the preview is empty. Then fetch member profiles (≤ 50 per call) and build/replace composite avatars via `should_replace`.
3. **Offline composite fallback** for groups still without any real member photo (see [groups.md](../api/groups.md#group-avatar-selection)).
4. After a full refresh, drop threads that still have the `"Người dùng "` placeholder name **and** no messages (unknown contacts with nothing to show).

Precedence of name sources: server group/friend name > profile name > placeholder > raw ID. The friend list's name overwrites a cached name whenever it is non-empty. [I]

## 6. Sidebar presentation

### 6.1 Last-message metadata  [I]
When recomputing, take the message with the greatest normalized timestamp. If it is not older than the stored `last_message_ts`, store its ts, ID, sender name, sender ID and outgoing flag. Sender ID = the wire sender field (or `senderId` of the history record) if it is a non-zero all-digit string. Keep a previously resolved (non-numeric) sender name when the same message comes back with only a numeric sender.

### 6.2 Preview text  [I] (matches official UI conventions)
```
text = preview with newlines → spaces; if empty → ""
sender/outgoing = from the newest message if it is newer than last_message_ts (or no stored sender), else stored metadata
sender = member name for that sender ID if known
if outgoing:                         "Bạn: " + text          (unless already prefixed)
elif group and sender non-empty and not all digits: "<sender>: " + text  (unless already prefixed)
else:                                text
```

### 6.3 Timestamp  [I]
- Activity time = max(normalized ts of all loaded messages, `last_message_ts`).
- If activity time > 0: elapsed < 24 h → `HH:MM`; else same calendar year → `dd/MM`; else `dd/MM/yyyy` (local time zone). The switch is at **24 elapsed hours**, not at local midnight. Otherwise show the stored `time` string.

### 6.4 Order and visibility  [I]
1. My Cloud and user-pinned threads first; then server-pinned (`pinned` metadata); then by newest message timestamp, descending.
2. Hidden threads are excluded unless a search query is active (searching by name or ID reveals them).
3. Filters: All / Direct (1-1) / Groups.
4. Row font is semibold when the thread has an unread badge (count > 0 or marked unread). Badge text: empty dot when muted or only marked-unread; `"5+"` for groups above 5; `"99+"` above 99; else the count.

## 7. Startup and refresh sequencing  [I]

### 7.1 After login or on app start with a saved session
1. Fetch server info (refresh endpoint map). Start the 1.2 s message-poll ticker, the websocket loop and the control long-poll loop (they idle while logged out).
2. Paint immediately from the local cache (thread snapshot + recent messages per thread from the message store).
3. Run a **full refresh** (below). After a fresh QR login the same steps run inline, then a phone/history sync is triggered ([../api/history-sync.md](../api/history-sync.md)).

### 7.2 Full refresh (startup, manual refresh)
1. **Session check.** Expired (`zpw_sek` rejected) → mark expired, log out, request a new QR. Unreachable → keep the session and the list, show "offline", retry on the next tick.
2. **Own profile** `me-v2` → update own UID/name.
3. **Load cache** (thread snapshot).
4. **Friends** `getfriends` → merge into list (name if non-empty; avatar via `should_replace`; `is_group` OR). The friend step also inserts My Cloud at the top, hydrates history from the store, merges groups (`getlg/v4` + `getmg-v2`) and runs name resolution.
5. **Groups** `getlg/v4` + `getmg-v2` → merge (name if non-empty; avatar via `should_replace`; `is_group = true`). (The previous client fetched groups twice — once inside step 4 and again here; one fetch is enough.)
6. **Hydrate history** from the local store.
7. **Messages**, with a fresh poll cursor:
   1. long-poll (≤ 3 batches while `more`/reset);
   2. `get-last-msgs` with `{}`, then for all known threads in chunks of 25;
   3. cloud messages (recent) for My Cloud and for **each group**;
   4. name resolution; drop placeholder threads without messages; dedup; persist to the store and cache.
   Then the fresh cursor replaces the shared poll cursor.
8. **Mute snapshot** `getmute` → apply (authoritative).
9. Publish the list. If the refresh was manual or produced zero messages, trigger a phone/history sync.

Per-step failures are collected and reported; the refresh continues with the remaining steps.

### 7.3 Steady state
- **Every 1.2 s** (skipped if a poll is in flight or logged out): long-poll messages (≤ 3 batches) into temporary threads for the known IDs/names; keep only threads with new messages; resolve names; publish as an update batch (§4.4). Session-expired errors trigger re-login; other errors are reported and retried.
- **Websocket frames:** extract VoIP and mute controls; for chat frames, route messages (with My Cloud known), keep threads with messages, resolve names, publish.
- **Control long-poll:** mute and VoIP events; back off 5 s after errors.
- **Opening a thread:** load more history from the store; for 1-1, `get-last-msgs` for that thread and with `{}`; for groups and My Cloud, recent cloud messages plus up to 5 older pages ([../api/history-sync.md](../api/history-sync.md)); resolve the group's name/avatar if still generic.
- **User actions** (pin, mark unread, mute, hide, delete): update local state and persist first, then fire the server call in the background ([../api/conversations.md](../api/conversations.md)). Mute additionally re-reads the server snapshot.
- **Logout:** delete the session and the thread cache, reset the poll cursor; the message store is kept unless the user wipes local data.

## Open questions

- [U] Does Zalo really issue a different user ID for the same contact after a re-login? The avatar-file-key merge heuristic depends on it; if IDs are stable, the heuristic should be dropped.
- [U] Generic-name detection by string prefix misclassifies real names starting with "Nhóm " / "Người dùng "; a rewrite should store an explicit placeholder flag.
- [U] My Cloud detected by name substring rather than `send2me_id`; the cross-thread "cloud pollution" cleanup is a workaround for a routing bug whose root cause was not identified.
- [U] Group thread IDs stored bare (`<grid>`) while mute map uses `g<grid>`: inconsistent key forms caused mismatches. Define one canonical key, e.g. `(kind, id)`.
- [U] Pin ordering mixes local pins, server-pinned metadata and My Cloud; server pins were never loaded, so pins made on other devices did not appear.
- [U] Sidebar sort uses only the last *loaded* message's timestamp, not `last_message_ts` metadata, so threads with metadata-only previews sort to the bottom — likely unintended.
- [U] Fetching recent cloud messages for every group on every full refresh may be expensive for users in many groups; the official client's strategy is unknown.
