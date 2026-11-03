# Media downloads and CDN

Where Zalo media (photos, files, video, voice, avatars) is served from, what the URLs look
like, how to name downloaded files, and the safety bounds a client should enforce when fetching
them. Recorded from the previous client (web session flavour, 2026-10).

Evidence tags: `[O]` observed / live-verified, `[I]` implemented in the previous client without
recorded live evidence, `[U]` unknown.

Related: [uploads.md](uploads.md) (how URLs are produced on send),
[stickers.md](stickers.md) (sticker images),
[../client-rules/message-model.md](../client-rules/message-model.md) (`msgType` codes, content shapes),
[../auth/session.md](../auth/session.md) (cookies, headers).

---

## 1. Where media URLs come from

Media is never fetched through an API service; each message carries absolute CDN URLs in its
`content` object. [I]

| Field in `content` | Meaning |
|---|---|
| `href` | Primary URL (photo full image, file download link, link target). |
| `thumb` | Thumbnail URL. |
| `hd` | HD image URL; used as thumbnail fallback when `thumb` is missing. |
| `title` / `fileName` / `name` | Display name; for files, the sender's original file name. |
| `description` / `caption` | Caption. |
| `params` | **A JSON string** that must be decoded again. Seen keys: `width`, `height` (photos); `fileSize` (string), `checksum`, `fileExt`, `fileName`, `mediaTitle`, `src` (files). |
| `fileExt` | File extension without dot (may also live inside `params`). |

- Additional URL fields observed by a raw-dump probe on received messages: `thumbUrl`,
  `normalUrl`, `oriUrl`, `hdUrl`, `rawUrl`. [I]
- Upload responses use `normalUrl`, `hdUrl`, `oriUrl`, `thumbUrl`, `url` (photos) and `fileUrl`
  (files); see [uploads.md](uploads.md). Preference order used when picking a URL: [I]
  - full image: `normalUrl` → `hdUrl` → `oriUrl` → `url`
  - HD: `hdUrl` → `normalUrl` → `oriUrl`
  - thumbnail: `thumbUrl` → `normalUrl` → `hdUrl`
  - file: `fileUrl` → `url` → `normalUrl` (→ `href` in echoes)
- Only accept `http://` / `https://` values; ignore empty strings. [I]
- `content` may itself be a JSON string; decode before reading. [I]

---

## 2. CDN hosts and URL shapes

All hosts below were seen in URLs carried inside messages or hardcoded by the bundle; none is a
service-map key. Only the shapes are known; the numbering schemes are server-assigned.
Placeholders replace real hashes/ids.

### 2.1 Photos

```
https://photo-stal-zmp.zdn.vn/jpg/<hash>/<id>.jpg           (full)
https://photo-stal-zmp.zdn.vn/jpg/<hash>/<id>_thumb.jpg     (thumb)   [I — test fixture shape]
```

Classification rule used to decide "this URL is a Zalo photo" (when `msgType` is ambiguous): [I]

```
is_photo_cdn(url):
    u = lowercase(trim(url))
    if not http(s) or u contains "/renewmedia": false
    p = u without query
    if p ends with any of .doc .docx .pdf .xls .xlsx .ppt .pptx .zip .rar .7z .txt .csv
                           .mp4 .mov .mp3 .m4a .exe .dmg .apk: false
    true if u contains "photo-stal" | "photo-zmp" | "photo-aka"
    true if u contains "zdn.vn" and (p contains /jpg/ /png/ /webp/ /gif/
                                     or p ends with .jpg .jpeg .png .webp .gif)
    else false
```

- `zpcloud.zdn.vn` hosts link-preview / cloud thumbnails that **expire**: when such a thumbnail
  fails to load, re-fetch the link preview to obtain a fresh thumbnail instead of retrying the
  old URL. [I]
- Photo messages have no usable text; the URL must not be shown as message text. Caption, if
  any, is in `description`. [I]

### 2.2 Files

```
https://f<n>-zpc.zdn.vn/renewMedia?p=<opaque>      [I — test fixture shape, #74]
```

- File messages (`share.file` / `chat.file`) point at a `/renewMedia` endpoint. The last path
  segment is literally `renewMedia`, so **the URL never contains the real file name** — see §4.
- Detect files by: message type is a file type, or `content` has `fileName`, or `params` has
  `fileExt`/`fileSize`/`checksum`, or the URL contains `/renewmedia` (case-insensitive). File
  detection wins over photo detection. [I]
- Whether `/renewMedia` URLs are signed/time-limited (the name suggests it re-issues a fresh
  link) is `[U]`.

### 2.3 Video and voice

- Video URLs: the previous client opened non-`/renewMedia` video URLs in the system browser and
  downloaded `/renewMedia` ones as files. Host and shape not recorded. [U]
- Voice: no URL shape recorded. [U]

### 2.4 Avatars

```
https://s120-ava-talk.zadn.vn/<n>/<32-hex>.jpg?key=<k>&time=<unix-seconds>
https://s160-ava-talk.zadn.vn/<n>/<32-hex>.jpg ...
https://s120-<m>-ava-talk.zadn.vn/<n>/<32-hex>.jpg ...
```
[I — shapes from test fixtures; real avatars returned by profile/group APIs.]

- Group avatars come from `fullAvt` (preferred) → `avt` → `avatar` in group info. [I]
- Other hosts seen in fixtures: `s120.ava.talk.zdn.vn`, `ava.zdn.vn`, cover photos on
  `cover-talk.zadn.vn`. [I]
- A URL ending in `/default` is Zalo's placeholder avatar ⇒ treat as "no avatar". [I]
- **Stable identity:** the file name (last path segment, ≥ 12 chars, ending `.jpg`/`.png`/`.webp`,
  lower-cased) is stable for one image. The `s120`/`s160` size prefix and the `key`/`time` query
  rotate between sessions and must be ignored when comparing two avatar URLs. [I]
- **Versioning:** `time=<digits>` is a server timestamp attached per session; a larger value means
  a newer issue of the URL. When two URLs name **different** files, prefer the one with the
  larger `time`. Whether `time` is an expiry and what happens after it passes is `[U]`. [I]
- Replacement rule used by the previous client (generic, independent of thread merging): [I]
  ```
  replace(current, incoming):
      if incoming empty: no
      if current empty: yes
      if count_real_urls(incoming) != count_real_urls(current): pick the larger
      if same file names (ignoring size prefix and query): no
      else: yes iff time(incoming) > time(current)
  ```
  where "real URL" = http(s) and not ending in `/default`.

---

## 3. Request requirements

- The previous client fetched all media with its session HTTP client, so every request carried
  the session cookie jar, `Origin: https://chat.zalo.me`, `Referer: https://chat.zalo.me/` and a
  Zalo PC Electron user agent. [I]
- Whether any CDN host **requires** cookies or Referer is unknown; nothing was observed failing
  without them because they were always sent. A rewrite should keep sending them for `zdn.vn` /
  `zadn.vn` hosts and verify. [U]
- Never send the session cookie to hosts outside the Zalo domains (link previews, arbitrary
  `href`s). The previous client did not distinguish; this is a recommended fix. [I]
- Send plain `GET`. No params encryption or `signkey` is involved for CDN downloads. [I]
- Some Zalo CDN responses are gzip bodies without `Content-Encoding` (seen on the sticker
  catalog); sniff `1F 8B`. [I]

---

## 4. Download file names

Zalo quirk: the URL for a file is `/renewMedia?...`, so naive "last path segment" naming saves
every file as `renewMedia` (#74). Resolve the name as follows: [I — regression-tested on a
fixture]

```
usable(s):  trimmed s non-empty, not http(s)://, not "renewmedia" (any case),
            not starting with "renewMedia." / "renewmedia.",
            not one of the client placeholders "[Tập tin]", "[Chia sẻ]", "[Hình ảnh]",
            "[Video]", "[Tin nhắn]"
strip_tag(s): remove a leading "[Tập tin]" or "[Chia sẻ]" and trim

name = first usable of:
         strip_tag(title)            # title / fileName / name / params.fileName / params.mediaTitle
         strip_tag(message text)
       else last path segment of URL (query removed),
            or "tap_tin_zalo" if empty or "renewMedia"

if fileExt known and name does not already end with "." + fileExt (case-insensitive):
    name += "." + fileExt          # title often lacks the extension

sanitize: replace / \ : * ? " < > | and control chars with "_"
          trim whitespace, then trim leading/trailing "."
          if empty → "tap_tin_zalo"
```

Saving: [I]
- Default folder `~/Downloads/Zalo Received Files` (Windows: `%USERPROFILE%\Downloads\Zalo Received Files`) unless the user set one.
- Never overwrite: on collision try `stem (1).ext`, `stem (2).ext`, … (split at the last `.` only
  when it is not the first char and not the last).
- Recommended additions not in the old download path: also reject `..`, Windows reserved names
  (`CON`, `NUL`, …) and over-long names; write to a temp file and rename on completion (§5).

---

## 5. Safety bounds (client policy)

These are not Zalo rules; they are the bounds the previous client designed for untrusted CDN
content. The bounded helpers existed but the main download paths had **not** been wired to them
yet (they read whole bodies with no limit). [I]

| Bound | Value |
|---|---|
| In-memory image / preview body | ≤ 16 MiB |
| HTML fetched for link previews | ≤ 128 KiB |
| Original file streamed to disk | ≤ 512 MiB |
| Decoded texture budget (LRU) | 128 MiB |
| On-disk media cache budget (LRU by mtime) | 512 MiB |
| Concurrent media jobs | 4 active, 128 queued; reject beyond |
| Preview fetch timeout | 20 s per request, 30 s total |
| Original download timeout | 120 s per request, 125 s total |
| Inline image downscale | longest side ≤ 720 px (viewer keeps full size) |

Rules: [I]
- Reject early if `Content-Length` exceeds the bound; still count bytes while streaming (the
  header can lie or be absent).
- Map HTTP 404 → "not found" (expired media), other non-2xx → HTTP error, timeouts and transport
  errors separately — expired CDN links are common.
- Cancellation: capture a generation ticket when starting; check it before the request, on every
  chunk and before committing; a stale ticket aborts and discards.
- Write originals to `<dest>.<ticket>.privatepart` (exclusive create, mode 0600), fsync, then
  atomic rename; delete the partial on any abort.
- Decode images by content sniffing, never by extension; a cached file that fails to decode is
  deleted and refetched.
- **Account isolation:** cache directory per account = `sha256(account id)` (mode 0700); cache
  entry name = `sha256(len‖account‖len‖thread‖len‖message‖quality)` + `.media`, quality ∈
  {thumbnail, preview, original}. Refuse to read another account's entry. Prune only files that
  match the cache naming pattern so user-saved files are never deleted.
- The previous GUI actually cached images by `thumb_|hd_|ava_` + `md5(url)` in a shared folder,
  not per account; the isolated scheme above is the intended design.

## Open questions

- Do CDN hosts (`*.zdn.vn`, `*.zadn.vn`) require the session cookie, `Referer` or a specific
  user agent? [U]
- Lifetime of `/renewMedia` URLs, avatar `key`/`time` tokens and `zpcloud.zdn.vn` thumbnails;
  is there an API to refresh an expired media URL? [U]
- Video and voice CDN hosts and URL shapes; where voice duration/format live in `content`. [U]
- Which of `thumbUrl`/`normalUrl`/`oriUrl`/`hdUrl`/`rawUrl` is actually present on received
  photos, and which is the true original. [U]
- Contradiction on numeric `msgType` for media: one mapping uses 2=photo, 3=voice, 4=video,
  5=sticker, 19=file; a media-hydration rule treats 2–4 as photo and 19 as video; the archive
  ("ji") mapping uses 3/4/31=photo, 6=voice, 10=sticker, 19=video, 22=file, 23=gif. Only string
  types (`chat.photo`, `share.file`, …) are reliable; see
  [../client-rules/message-model.md](../client-rules/message-model.md). [U]
- Meaning of the `checksum` field in file `params` (algorithm) and whether clients should verify
  downloads against it. [U]
