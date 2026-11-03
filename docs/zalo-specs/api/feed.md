# Social feed (Nhật ký) and social profiles

Endpoints for a friend's social profile (cover, avatar, birthday, status) and the user's own timeline photo album, plus what is and is not known about the real social feed (Nhật ký) timeline.

Evidence tags: `[O]` observed / live-verified, `[I]` implemented in the previous client without recorded live evidence, `[U]` unknown.

Related docs: [../security/payload-encryption.md](../security/payload-encryption.md), [../auth/session.md](../auth/session.md), [../auth/service-discovery.md](../auth/service-discovery.md), [../realtime/websocket.md](../realtime/websocket.md), [../client-rules/errors.md](../client-rules/errors.md).

---

## 1. Status summary

| Capability | Status |
|---|---|
| Single friend social profile (`getprofiles`) | `[I]` |
| Batch friend social profiles (`getprofiles/v2`) | `[I]` |
| Own avatar / timeline album (`avatar-list`) | `[I]` |
| Real feed posts (text, media, likes, comments) | **No working API known** `[U]` |
| Post / edit feed | **No working API known** `[U]` |

`[I]` The previous client's "timeline" screen did **not** read real posts. It synthesised one card per friend from social-profile fields (status text, cover, avatar, birthday) and from photos already shared in chat, ordered by `lastActionTime`, and invented like/comment counts and relative dates locally. A re-implementer must not treat that as protocol behaviour.

---

## 2. Endpoints

### Get friend social profile
- **Request:** `GET {profile}/api/social/friend/getprofiles?zpw_ver=<ver>&zpw_type=<type>&params=<enc>&os=8`  (default host: `https://tt-profile-wpa.chat.zalo.me`)
- **Encoding:** session AES (`zpw_enk`), URL-encoded into `params`; no `signkey`. `[I]`
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `avatar_size` | int | yes | Requested avatar size in px; previous client sends `240` |
  | `friend_ids` | array of string | yes | Target user ids (previous client sends exactly one) |
  | `imei` | string | yes | `<imei>` |
- **Response (`data` after decryption):** object keyed by user id: `data["<uid>"]` = profile object (§3). `[I]`
- **Errors:** previous client treats anything other than `error_code == 0` with `data["<uid>"]` present as failure; no specific codes recorded. `[U]`
- **Notes:** the extra query parameter `os=8` is appended after `params` `[I]`; its meaning is `[U]`.

### Get friend social profiles (batch, v2)
- **Request:** `POST {profile}/api/social/friend/getprofiles/v2?zpw_ver=<ver>&zpw_type=<type>`  (default host: `https://tt-profile-wpa.chat.zalo.me`), body `application/x-www-form-urlencoded`: `params=<enc>`
- **Encoding:** session AES (`zpw_enk`); no `signkey`. `[I]`
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `phonebook_version` | int | yes | `0` |
  | `friend_pversion_map` | array of string | yes | One entry per target, `"<uid>_<profileVersion>"`; sending version `0` forces the server to return the profile |
  | `avatar_size` | int | yes | `240` |
  | `language` | string | yes | `"vi"` |
  | `show_online_status` | int | yes | `1` |
  | `imei` | string | yes | `<imei>` |
- **Response (`data` after decryption):** `data.changed_profiles` = object keyed by uid → profile object (§3). The previous client also accepts `data` itself being that map. `[I]` Profiles whose version matches are presumably omitted (by analogy with version maps elsewhere) `[U]`.
- **Errors:** none recorded. `[U]`
- **Notes:** `[I]` client policy: at most 40 ids per call; if the batch returns nothing, fall back to the single-profile call for the first 8 ids. `[U]` server maximum batch size.

### Get own avatar / timeline album
- **Request:** `GET {profile}/api/social/avatar-list?zpw_ver=<ver>&zpw_type=<type>&params=<enc>&os=8`  (default host: `https://tt-profile-wpa.chat.zalo.me`)
- **Encoding:** session AES (`zpw_enk`); no `signkey`. `[I]`
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `page` | int | yes | Page number; previous client starts at `1` |
  | `count` | int | yes | Items per page; previous client uses `20` |
  | `imei` | string | yes | `<imei>` |
- **Response (`data` after decryption):** `data.photos[]`, each `{ photoId: string, url: string, thumbnail?: string }`. Missing `thumbnail` → use `url`. Entries with empty `url` are skipped. `[I]` Media are served from the social CDN (`zpsocial`). `[I]`
- **Errors:** none recorded. `[U]`
- **Notes:** whether this returns avatar history only or all timeline photos is `[U]`; termination of paging (empty page? total field?) is `[U]`. Only the logged-in user's own album; no friend variant known.

---

## 3. Social profile object

`[I]` Fields read from each profile entry (all optional):

| Field | Type | Meaning / fallback |
|---|---|---|
| `displayName` | string | Display name; fallback the uid |
| `zaloName` | string | Account name; fallback `displayName` |
| `username` | string | Username |
| `avatar` | string (URL) | Avatar |
| `cover` | string (URL) | Cover photo |
| `gender` | int | Gender code (`0` default; mapping `[U]`) |
| `sdob` | string | Birthday as display string (e.g. `DD/MM/YYYY`; may be partial/hidden `[U]`) |
| `status` / `statusMsg` | string | Bio / status text (`status` preferred) |
| `isFr` | int | `1` = is a friend |
| `lastActionTime` | int | Last activity time (unit, presumably epoch ms, `[U]`) |

`[I]` An avatar or cover URL ending in `/default` denotes the placeholder image (no custom photo set).

---

## 4. Feed timeline over the socket (unverified)

`[U]` The previous client contained a builder for a "feed timeline" binary request identified as command **50001**, but it was **never sent** to any server; it was only rendered as hex in a debug screen. Its framing (`u32 BE length`, `u16 BE cmd`, `u32 BE subcmd`, binary body) does not match the websocket frame format documented in [../realtime/websocket.md](../realtime/websocket.md) (1-byte version, `u16 LE` cmd, 1-byte sub-command, JSON body). No capture or bundle reference backs it. The constants are recorded only so they can be checked against a real capture:

- sub-commands: get feed `0x00150102`, post feed `0x00220411`, edit feed `0x26580022`
- get-feed body (51 bytes, big-endian): `0x01`, `feed_version u32 = 2`, `tab u32`, `page u32`, `count u32`, `last_feed_id_server u64 = 0`, `last_feed_id_local u64 = 0`, `owner_id u64`, `viewer_id u64`, `filter_flags u16 = 0`

Treat all of §4 as unverified speculation until confirmed.

---

## Open questions

- `[U]` The real feed API (Nhật ký posts, reactions, comments, posting): REST path, service key, or socket command. Nothing in hand is evidenced.
- `[U]` Meaning of `os=8` on the profile GET endpoints and whether the POST v2 call needs it.
- `[U]` `avatar-list` scope and paging termination.
- `[U]` `gender` code mapping, `lastActionTime` unit, `sdob` format variants.
- `[U]` Whether `getprofiles/v2` omits unchanged profiles and its batch limit.
- Contradiction: the command-50001 packet format conflicts with the documented websocket frame format; it is most likely not a real Zalo format.
