# Contacts: friends, profiles, phone lookup, friend requests, online status

How a client lists friends, resolves user display names and avatars, finds a user by phone number, manages friend requests and reads a contact's last-online time.

Evidence tags: `[O]` observed (live-verified, or read from the shipped official Zalo PC bundle); `[I]` implemented in the previous client without recorded live evidence; `[U]` unknown.

## Conventions shared by every call in this doc

- **Common query params:** every URL carries `zpw_ver=<api version>&zpw_type=30` and nothing else is common. [O] See [../auth/session.md](../auth/session.md).
- **Encoding "session AES":** the plaintext param object is serialized to JSON, encrypted with the session key `zpw_enk` (AES-CBC, zero IV, PKCS7, base64), URL-encoded and sent as `params=<…>` — in the query string for `GET`, in an `application/x-www-form-urlencoded` body for `POST`. No `signkey` is added on these calls. [I] Algorithm: [../security/payload-encryption.md](../security/payload-encryption.md).
- **Response decryption:** when the envelope's `data` is a base64 string (not starting with `{`), decrypt it with the same key; the plaintext is itself a full envelope `{error_code, error_message, data}` and its `error_code` is authoritative. [I]
- **Tolerant `data`:** after decryption, `data` may still be a JSON-encoded *string*; parse it again. Some responses nest the payload one level deeper as `data.data`. [I]
- **Auth:** all calls require the `zpw_sek` session cookie; without it the server answers `error_code 600`. [O] See [../auth/session.md](../auth/session.md) and [../client-rules/errors.md](../client-rules/errors.md).
- **Hosts:** service keys come from the service map, see [../auth/service-discovery.md](../auth/service-discovery.md). For `getfriends` the previous client tried the `friend` host candidates first and fell back to the `profile` host when they failed at transport level or with HTTP 404 (business errors never fail over). The other friend calls used the `friend` host only. [I]
- **Thread IDs:** a user ID is a decimal string (`<uid>`). Groups are not covered here; see [groups.md](groups.md).

---

## Friend list

### Get friends
- **Request:** `GET {friend}/api/social/friend/getfriends?zpw_ver=…&zpw_type=30&params=…` (default host: `https://tt-friend-wpa.chat.zalo.me`)
- **Encoding:** session AES (`zpw_enk`). If no session key is available the previous client sent the same object as plain URL-encoded JSON in `params`. [I]
- **Params (plaintext, before encryption):**

  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `incInvalid` | int | yes | `1` [I] — include invalid/deactivated accounts (meaning inferred from name) [U] |
  | `page` | int | yes | `1` [I] |
  | `count` | int | yes | `20000` — page size large enough to fetch everything in one page [I] |
  | `avatar_size` | int | yes | `160` (pixel size of returned avatar URLs) [I] |
  | `actiontime` | int | yes | `0` [I] — presumably "changes since"; `0` = full list [U] |
  | `imei` | string | yes | client device ID `<imei>` |

- **Response (`data` after decryption):** one of these shapes, all accepted by the previous client [I]:
  - an array of friend objects, or
  - `{ "friends": [ … ] }`, or
  - `{ "data": [ … ] }`.

  Friend object fields used (first non-empty key wins):

  | Purpose | Keys, in priority order |
  |---|---|
  | user ID | `userId`, `uid`, `id`, `friendId`, `globalId` |
  | display name | `displayName`, `zaloName`, `display_name`, `name`, then `profile_displayName`, then `profile.displayName`; fall back to the ID |
  | avatar URL | `avatar`, `avt`, `profile_avatar` |
  | group flag | `isGroup` (`true` or `1`) — friends should never carry it |
  | last action time | `lastActionTime` (number or numeric string; seconds or ms) |

  Values may be strings or numbers; a number must be stringified. [I]
- **Errors:** generic only (`600` session invalid). [O]
- **Notes:**
  - [O] The official client's registry lists `/api/social/friend/getfriends` among the startup bootstrap calls (see [conversations.md](conversations.md)).
  - [U] An earlier host table listed this endpoint under the `profile` host; the previous client used the `friend` host first and fell back to `profile`. Both appear to work but this was not compared on a capture.
  - [U] No pagination beyond `page=1, count=20000` was ever exercised.
  - The friend list is also the main source of 1-1 conversation entries; see [conversations.md](conversations.md) and [../client-rules/threads.md](../client-rules/threads.md).

---

## Profiles

### Get user profiles (names + avatars for arbitrary UIDs)
- **Request:** `POST {profile}/api/social/group/members?zpw_ver=…&zpw_type=30` (default host: `https://tt-profile-wpa.chat.zalo.me`); body `params=…`
- **Encoding:** session AES (`zpw_enk`)
- **Params (plaintext, before encryption):**

  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `friend_pversion_map` | string[] | yes | one entry per user, `"<uid>_<profileVersion>"`; send `"<uid>_0"` to always get the current profile. An entry already containing `_` is sent unchanged. |

- **Response (`data` after decryption):** `data.profiles` is a map `uid → profile` (some responses use `data.data.profiles`, or `data` is the map itself) [I]. Profile fields used:

  | Purpose | Keys, in priority order |
  |---|---|
  | name | `zaloName`, `displayName`, `name`, `display_name` |
  | avatar URL | `avatar`, `avt`, `profile_avatar` |

- **Errors:** generic only.
- **Notes:**
  - Command ID `12030` [I] (from the previous client's notes, not re-checked).
  - [I] Batch size used: at most **50** UIDs per request; larger lists are chunked.
  - [I] The previous client recorded that `/api/social/friend/getminiprofiles` returns `error_code 114` when given a raw UID list, while this endpoint with `"<uid>_0"` entries returns `error_code 0`. No capture was kept. Earlier notes named `getminiprofiles` / `/api/friend/profile/multiget` as the profile source.
  - Used for: 1-1 threads with placeholder names, unknown senders discovered by polling, and member photos for composite group avatars (see [groups.md](groups.md)).

### Get own profile
- **Request:** `GET {profile}/api/social/profile/me-v2?zpw_ver=…&zpw_type=30&params=…&os=8`
- **Encoding:** session AES (`zpw_enk`)
- **Params (plaintext):** `avatar_size` = `160`, `imei` = `<imei>`. [I]
- **Response:** `data.profile` (or top-level `profile`) with `userId`|`uid`, `displayName`|`zaloName`, avatar. [I]
- **Notes:** called on every refresh to re-sync the logged-in user's UID and name before loading conversations. [I] Owned by [../auth/session.md](../auth/session.md); listed here only because the conversation refresh depends on it.

---

## Find a user by phone number

### Phone normalisation (client side, before the request)  [I]

```
normalize(raw):
  digits = keep only ASCII digits of raw          # drops '+', spaces, '.', '-', '(', ')'
  if digits starts with "0": return "84" + digits[1:]   # Vietnamese national → international
  return digits                                      # already international, e.g. "84…" or another country code
```

- Reject the input when the normalized string is shorter than **8** characters. [I] (The user-facing error says "at least 9 digits" — see Open questions.)
- Examples: `0912 345 678` → `84912345678`; `+84 912-345-678` → `84912345678`; `+1 555 0100` → `15550100`.
- Only a leading `0` is rewritten; other national formats are passed through untouched. [I]

### Get user by phone
- **Request:** `GET {friend}/api/friend/profile/get?zpw_ver=…&zpw_type=30&params=…` (default host: `https://tt-friend-wpa.chat.zalo.me`)
- **Encoding:** session AES (`zpw_enk`)
- **Params (plaintext, before encryption):** [O] (official `getUserByPhone`, cmd `11420`)

  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `phone` | string | yes | normalized phone (see above) |
  | `avatar_size` | int | yes | `240` |
  | `language` | string | yes | `"vi"` |
  | `imei` | string | yes | `<imei>` |
  | `reqSrc` | int | yes | `40` (request source = search by phone) |

- **Response (`data` after decryption):** a single profile object [I]:

  | Purpose | Keys, in priority order |
  |---|---|
  | user ID | `uid`, `userId`, `id`, `globalId` |
  | display name | `display_name`, `displayName`, `zalo_name`, `zaloName` (fallback: the raw phone input) |
  | Zalo name | `zalo_name`, `zaloName`, `display_name`, `displayName` |
  | avatar | `avatar`, `avt` |
  | status text | `status` |

  A response without any user-ID key means "not found". [I]
- **Errors:**

  | `error_code` | Meaning |
  |---|---|
  | `216` | phone not registered on Zalo, or the owner disabled "find me by phone" [I] |
  | other non-zero | show `error_message` [I] |

- **Notes:** after a hit, the previous client immediately called [Get friendship status](#get-friendship-status) for the returned UID so the UI can show "Add friend / Pending / Accept / Message". [I]

---

## Friend requests

All five calls use the `friend` service (default host `https://tt-friend-wpa.chat.zalo.me`) and session AES encoding. Command IDs are from the previous client's notes referencing the official bundle. [I]

### Get friendship status
- **Request:** `GET {friend}/api/friend/reqstatus?zpw_ver=…&zpw_type=30&params=…` (cmd `11430`)
- **Encoding:** session AES (`zpw_enk`)
- **Params (plaintext):**

  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `fid` | string | yes | other user's `<uid>` |
  | `imei` | string | yes | `<imei>` |

- **Response (`data` after decryption; may be nested in `data.data`):** flags, each an int `0/1` or a boolean, in snake_case or camelCase:

  | Field | Meaning |
  |---|---|
  | `is_friend` / `isFriend` | already friends [O] (official bundle) |
  | `is_requested` / `isRequested` | **I** sent them a request that is pending [O] (official bundle) |
  | `is_requesting` / `isRequesting` | **they** sent me a request that is pending [O] (official bundle) |

- **Client mapping (evaluate in this order):** `is_friend=1` → Friend; else `is_requesting=1` → IncomingPending; else `is_requested=1` → OutgoingPending; else Stranger. On request failure → Unknown. [I]
- **Errors:** generic only.

### Send friend request
- **Request:** `POST {friend}/api/friend/sendreq?zpw_ver=…&zpw_type=30` (cmd `11424`); body `params=…`
- **Encoding:** session AES (`zpw_enk`)
- **Params (plaintext):**

  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `toid` | string | yes | target `<uid>` |
  | `msg` | string | yes | greeting text shown to the recipient (may be empty) [I] |
  | `reqsrc` | int | yes | `30` [I] — request source; meaning of `30` not documented [U] |
  | `imei` | string | yes | `<imei>` |
  | `language` | string | yes | `"vi"` |

- **Response:** success envelope; payload unused. [I]
- **Errors:** any non-zero `error_code` is a failure; specific values unknown. [U]
- **Notes:** on success the client sets local status to OutgoingPending. [I]

### Accept friend request
- **Request:** `POST {friend}/api/friend/accept?zpw_ver=…&zpw_type=30` (cmd `11427`); body `params=…`
- **Encoding:** session AES (`zpw_enk`)
- **Params (plaintext):** `fid` (string, the requester's `<uid>`), `language` = `"vi"`. **No `imei`.** [I]
- **Response:** success envelope; payload unused. On success local status → Friend. [I]
- **Errors:** any non-zero code = failure. [U] specific values.

### Undo (cancel) my outgoing request
- **Request:** `POST {friend}/api/friend/undo?zpw_ver=…&zpw_type=30` (cmd `11441`); body `params=…`
- **Encoding:** session AES (`zpw_enk`)
- **Params (plaintext):** `fid` (string, target `<uid>`), `imei`. [I]
- **Response:** success envelope; on success local status → Stranger. [I]

### Reject an incoming request
- **Request:** `POST {friend}/api/friend/reject?zpw_ver=…&zpw_type=30` (cmd `11428`); body `params=…`
- **Encoding:** session AES (`zpw_enk`)
- **Params (plaintext):** `fid` (string, requester `<uid>`), `imei`. [I]
- **Response:** success envelope; on success local status → Stranger. [I]

### Listing pending requests
- [U] No endpoint for listing received/sent friend requests was implemented. The previous client's Contacts page had a "friend requests" section but populated it only from statuses it had queried one by one.

---

## Online status

### Get last online
- **Request:** `GET {profile}/api/social/profile/lastOnline?zpw_ver=…&zpw_type=30&params=…` (cmd `12090` [I]) (default host: `https://tt-profile-wpa.chat.zalo.me`)
- **Encoding:** session AES (`zpw_enk`)
- **Params (plaintext):**

  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `uid` | string | yes | contact `<uid>` (trimmed) |
  | `conv_type` | int | yes | `1` [I] — 1-1 conversation |
  | `imei` | string | yes | `<imei>` |

- **Response (`data` after decryption; may be a JSON string or nested in `data.data`):**

  | Field | Type | Meaning |
  |---|---|---|
  | `settings.show_online_status` | bool or int | user allows showing online status; missing = treat as allowed [I] |
  | `lastOnline` | int or numeric string | last activity time; values > 10¹⁰ are milliseconds, otherwise seconds [I] |

- **Client rule (mirrors official `getLastActiveText`, `user_active_time` = 5 minutes):** [I]
  ```
  if show_online_status is false or lastOnline <= 0: show nothing
  diff = max(0, now_sec - lastOnline_sec)
  diff <= 300           → online,  "Vừa mới truy cập"  (just active)
  diff < 3600           → offline, "Truy cập <min> phút trước"
  diff < 86400          → offline, "Truy cập <hours> giờ trước"
  else                  → offline, "Truy cập <min(days,7)> ngày trước"
  ```
- **Errors:** generic only.
- **Notes:** queried on demand when a contact's profile/chat header is opened, together with friendship status. Not polled. [I] Realtime presence pushes over the websocket were not used. [U]

## Open questions

- [U] Meaning of `incInvalid`, `actiontime` in `getfriends`, and whether pagination is ever needed.
- [U] Which host is canonical for `getfriends` (`friend` vs `profile`) — sources disagree.
- [U] `getminiprofiles` returning `114` for raw UID lists is recorded but not captured; the exact parameter shape that the official client sends to `getminiprofiles` is unknown.
- [U] Phone validation contradiction: the length check is `< 8` characters of the normalized number, but the user message says "at least 9 digits". The intended minimum is unknown.
- [U] Error codes for `sendreq` / `accept` / `undo` / `reject` (already friends, request limit reached, blocked, etc.).
- [U] Meaning of `reqsrc = 30` (sendreq) vs `reqSrc = 40` (phone lookup); whether other sources change server behaviour.
- [U] Why `accept` carries no `imei` while the other friend calls do — copied from the official bundle or an omission.
- [U] Endpoint for listing pending friend requests (incoming/outgoing) and for blocking/unfriending.
- [U] Realtime friend-request / presence events on the websocket (see [../realtime/websocket.md](../realtime/websocket.md)).
