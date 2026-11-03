# Groups: group list, group info, members and display metadata

How a client lists the groups the user belongs to, fetches each group's name, avatar and roster, and derives the metadata it needs to render a group in the conversation list.

Evidence tags: `[O]` observed (live-verified, or read from the shipped official Zalo PC bundle); `[I]` implemented in the previous client without recorded live evidence; `[U]` unknown.

Common params, session-AES encoding, response decryption and tolerant `data` parsing are the same as in [contacts.md](contacts.md#conventions-shared-by-every-call-in-this-doc). All calls use the `group` service (default host `https://tt-group-wpa.zaloapp.com`) unless stated.

## Group ID forms

- On the wire a group is identified by its bare numeric **grid** (`<grid>`). [O]
- Some contexts use a prefixed form: `g<grid>` (conversation keys, mute map, pin list) or `group_<grid>` (legacy). Before sending a `grid` the client strips a leading `group_`, then a leading `g`/`G`. [O] (official `getGroupIdFromConversationId` strips the prefix)
- The previous client stored group threads under the bare `<grid>` with an `is_group` flag. [I] See [../client-rules/threads.md](../client-rules/threads.md) for the consequences.

---

### List my groups
- **Request:** `GET {group}/api/group/getlg/v4?zpw_ver=…&zpw_type=30` (cmd `12027` [I])
- **Encoding:** plain — no `params` at all. [I]
- **Params:** none.
- **Response (`data` after decryption; may be a JSON string or nested in `data.data`):**

  | Field | Type | Meaning |
  |---|---|---|
  | `gridVerMap` | object `grid → version` | every group the user is a member of; the value is the group's metadata version [I] |

- **Errors:** generic only.
- **Notes:**
  - [I] The list carries **no names or avatars**; follow it with [Get group info](#get-group-info) for every grid.
  - Until info arrives, the client used the placeholder name `"Nhóm <grid>"` (see generic names in [../client-rules/threads.md](../client-rules/threads.md)).
  - [U] What the version value is used for by the official client (incremental info refresh, probably together with `getmg-v2`'s `gridVerMap`).

### Get group info
- **Request:** `POST {group}/api/group/getmg-v2?zpw_ver=…&zpw_type=30`; body `params=…`
- **Encoding:** session AES (`zpw_enk`)
- **Params (plaintext, before encryption):**

  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `gridVerMap` | string (JSON-encoded object) | yes | `{"<grid>": <knownVersion>, …}`; serialized to a JSON **string** inside the param object |
  | `imei` | string | yes | `<imei>` |

- **Response (`data` after decryption; may be a JSON string or nested in `data.data`):**

  | Field | Type | Meaning |
  |---|---|---|
  | `gridInfoMap` | object `grid → info` | full info for groups whose version changed since the version you sent |
  | `unchangedsGroup` | array | groups whose version matched — no info returned [I] |
  | `removedsGroup` | string[] | grids the user is no longer in (left/kicked/disbanded); drop them from the list [I] |

  Group `info` fields used:

  | Field | Type | Meaning |
  |---|---|---|
  | `name` (fallbacks `groupName`, `displayName`) | string | group display name |
  | `fullAvt` (fallbacks `avt`, `avatar`) | string | group avatar URL; a URL ending in `/default` means "no custom avatar" |
  | `totalMember` | int or numeric string | server member count — the **only** trusted source of member count |
  | `memVerList` | string[] | members as `"<uid>_<profileVersion>"`; UID = text before the first `_` |
  | `memberVerMap` | object `uid → version` | alternative roster shape; used only if `memVerList` is absent |
  | `creatorId` | string | owner UID |
  | `adminIds` | string[] (or numbers) | deputy admin UIDs |
  | `topMember` | array of profiles | a few member profiles: `id`/`userId`/`uid`, `displayName`/`zaloName`/`name`, `avatar`/`avt` |

- **Errors:** generic only.
- **Notes:**
  - [I] **Always send version `0`** for each grid. With the real version the server may put the group in `unchangedsGroup` and return no info, which leaves a freshly started client without a name.
  - [I] Batch size: at most 50 grids per call when resolving placeholder groups; at startup all grids from `getlg/v4` were sent in one call (size limits untested [U]).
  - [I] The roster fields above were "observed in the original getmg-v2 consumer" of the official client (bundle reading), not on a capture.

### Group members' profiles
- Member names/avatars are not in `getmg-v2` (beyond `topMember`). Fetch them with the profile batch call `POST {profile}/api/social/group/members` using `friend_pversion_map: ["<uid>_0", …]`, max 50 UIDs per call. Full spec in [contacts.md](contacts.md#get-user-profiles-names--avatars-for-arbitrary-uids). [I]

### Group message history
- `GET {group}/api/group/history` and the cloud-message endpoints `/api/cm/getrecentv2` / `/api/cm/getoldv2` belong to [history-sync.md](history-sync.md). [O] (registry)

### Delete group conversation (for me)
- Specified with the 1-1 variant in [conversations.md](conversations.md#delete-conversation-history-for-me).

---

## Display metadata derived from group info (client rules)

### Member ordering  [I]

```
ids = unique UIDs from memVerList (text before "_"), in server order
      (or the keys of memberVerMap if memVerList is absent)
ordered = []
for id in [creatorId] + adminIds + ids:
    if id in ids and id not in ordered: ordered.append(id)
for p in topMember:                          # sidebar roster only
    if p.id not in ordered: ordered.append(p.id)
```

- Creator first, then admins, then everyone else — only if they are actually in the roster. [I]
- Member names/avatars from `topMember` are applied when present; otherwise keep any previously known name/avatar for that UID. [I]
- **Member count** shown to the user = `totalMember` only. Never infer it from message senders or from the size of the avatar sample. [I] (For the composite avatar's "+N" badge the client used `max(totalMember, number of known member IDs)`.)

### Group avatar selection  [I]

1. If `info.fullAvt` / `avt` / `avatar` is a non-empty URL **not** ending in `/default` → use it.
2. Otherwise build a **composite avatar** from up to 4 members:
   - take the first 4 UIDs in [member order](#member-ordering);
   - fetch their profiles (batch, ≤ 50);
   - each slot = `(name, avatarUrl)`: name from the profile (`zaloName`, `displayName`, `name`, `display_name`), else the i-th comma-separated part of the group name, else `"Thành viên"`; avatar URL only if non-empty and not ending in `/default`;
   - if the group name looks like a comma-separated member list (≥ 2 parts), reorder slots so slot names match the order of names in the group name (accent-insensitive equality or substring);
   - total = `max(totalMember, number of slots)`.
3. Offline fallback: when a group has no real member photo yet and its name is a comma-separated list of ≥ 2 names, match each name (accent-insensitive, exact) against the names of known 1-1 contacts with a real avatar and use their photos for up to 4 slots; total = number of name parts.

The previous client serialized a composite avatar as a string:

```
multi:<total>;<name1>|<url1>;<name2>|<url2>;…
```
with `;` and `|` removed/replaced inside names and URLs; an empty URL means "draw initials". This is a client-internal format, not a server format. Replacement rules between avatars are in [../client-rules/threads.md](../client-rules/threads.md#avatar-selection-and-replacement).

### Group preview placeholder  [I]

If a group has no last-message preview yet and `totalMember > 0`, the sidebar preview shows `"Nhóm · <totalMember> thành viên"`.

### Removed groups  [I]

Drop every grid listed in `removedsGroup`. Groups that appear only in `clearUnreads` of the last-message response (see [conversations.md](conversations.md)) must **not** create a sidebar entry, because disbanded groups linger there; active groups come only from `getlg/v4`.

## Open questions

- [U] Meaning/use of the `gridVerMap` version values and the exact contents of `unchangedsGroup`.
- [U] Max number of grids accepted per `getmg-v2` call.
- [U] Whether `getlg/v4` requires `params` (e.g. `imei`) in the official client; the previous client sent none and it worked for it, but no capture was kept.
- [U] Endpoints for group management (create, rename, add/remove members, leave, admin changes) and group invitations were not implemented.
- [U] Realtime group events (member joined/left, renamed, avatar changed) and how they update cached group info.
- [U] Whether `topMember` order is meaningful (e.g. most active members).
