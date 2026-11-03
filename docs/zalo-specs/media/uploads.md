# Attachment uploads

How a client uploads and sends photos, GIFs, videos and generic files to 1-1 and group conversations: chunked upload endpoints, multipart fields, the follow-up send request, fallbacks and local display rules. Stickers are in [stickers.md](stickers.md); downloading received media is in [downloads.md](downloads.md).

Evidence note: the two-stage flow is modelled on Zalo PC 26.10.11's web bundle (`photo_original/upload` → `photo_original/send`, `asyncfile/upload` → `asyncfile/msg`) with single-shot fallbacks (`message/photo`, cmd `11152`; `sharefile_full`, cmd `11146`). It was implemented in the previous client; no request/response capture was recorded. Everything here is `[I]` unless marked otherwise.

## 1. Common mechanics

- **Host:** service key `file`, default `https://tt-files-wpa.zaloapp.com` [O — host answered in a live session]. Both the upload **and** the follow-up send go to the `file` host (not `chat` / `group`). [I]
- **Encoding:** session AES (`zpw_enk`) — `params` = URL-encoded base64 of `AES(zpw_enk, JSON)`. No ParamCipher v2, no signkey. See [../security/payload-encryption.md](../security/payload-encryption.md). [I]
- **Auth headers:** `Cookie: zpw_sek=<sek>; _zlang=vi`, header `zpw_sek: <sek>`, `Origin` / `Referer` `https://chat.zalo.me`. See [../auth/session.md](../auth/session.md). [I]
- **Two request shapes:** [I]
  - *Upload / single-shot* requests: `POST ...?zpw_ver=<ver>&zpw_type=30&params=<enc>&type=<t>` — the encrypted params are in the **query string**; the body is `multipart/form-data`.
  - *Send* requests (`photo_original/send`, `asyncfile/msg`): `POST ...?zpw_ver=<ver>&zpw_type=30` with form body `params=<enc>` (same as text sends, [../api/messages-send.md](../api/messages-send.md) §1).
- **`type` query value:** `2` for 1-1, `11` for group. [I]
- **Target field:** `toid` (1-1) or `grid` (bare group ID, group), as in [../api/messages-send.md](../api/messages-send.md). [I]
- **`clientId`:** allocated once per attachment (rule in [../api/messages-send.md](../api/messages-send.md) §1.3) and reused for every chunk, the send request and any fallback. [I]
- **Responses:** envelope; `data` may be an encrypted string to decrypt with `zpw_enk`. An empty HTTP 200 body was treated as success with `data: null`. [I]

### 1.1 Multipart body  [I]

```
Content-Type: multipart/form-data; boundary=----WebKitFormBoundary<16 random hex>

--<boundary>\r\n
Content-Disposition: form-data; name="<field>"; filename="<file name>"\r\n
Content-Type: <mime>\r\n
\r\n
<bytes>\r\n
--<boundary>--\r\n
```

- `<field>` is `chunkContent` for chunked uploads, `fileContent` for single-shot uploads.
- In the filename, `"`, CR and LF were replaced by `_`. Non-ASCII names were sent raw (UTF-8). Whether Zalo expects RFC 5987 encoding is `[U]`.
- The previous client used a 90 s timeout per multipart request.

### 1.2 Kind selection by extension  [I]

Lower-cased file extension:

| Kind | Extensions | Flow |
|---|---|---|
| Photo (incl. GIF) | `png jpg jpeg webp gif bmp` | §2 |
| Video | `mp4 mov mkv webm avi` | §3 with `fType: 2` |
| Anything else (docs, audio, archives) | — | §3 with `fType: 1` |

MIME types sent: `png→image/png`, `webp→image/webp`, `gif→image/gif`, `bmp→image/bmp`, other images `image/jpeg`; `mp4→video/mp4`, `mov→video/quicktime`, `mkv→video/x-matroska`, `webm→video/webm`, `mp3→audio/mpeg`, `wav→audio/wav`, `pdf→application/pdf`, `zip→application/zip`, `txt/md/log→text/plain`, else `application/octet-stream`. (`avi` falls to `application/octet-stream`.)

- **GIF** has no dedicated endpoint here: it is uploaded through the photo flow. Whether Zalo PC uses a separate GIF/animated path is `[U]`.
- **Voice messages** were not implemented for sending. Endpoint and payload `[U]`.

### 1.3 Chunking  [I]

- Chunk size: **1 MiB** (1 048 576 bytes). `totalChunk = ceil(size / 1 MiB)`, minimum 1.
- `chunkId` is **1-based**; chunks are uploaded sequentially, in order.
- Every chunk request carries the same JSON params except `chunkId`.
- Only the **last** chunk's response is used to obtain the uploaded object's ID/URLs; intermediate responses were ignored. Whether the server returns the final ID only on the last chunk, or asynchronously (e.g. via a realtime event), is `[U]`.
- Empty files (0 bytes) are rejected client-side before any request.
- No client-side size limit was enforced. Server limits are `[U]`.

## 2. Photos

### Upload photo chunk
- **Request:** `POST {file}/api/message/photo_original/upload?zpw_ver=<ver>&zpw_type=30&params=<enc>&type=2` (1-1) or `POST {file}/api/group/photo_original/upload?...&type=11` (group)  (default host: `https://tt-files-wpa.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`) in query; body multipart, field `chunkContent`
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `totalChunk` | int | yes | Number of chunks |
  | `fileName` | string | yes | Original file name |
  | `clientId` | int | yes | Shared attachment client ID |
  | `totalSize` | int | yes | Whole-file size in bytes |
  | `imei` | string | yes | Login IMEI |
  | `isE2EE` | int | yes | `0` |
  | `jxl` | int | yes | `0` (JPEG XL off) |
  | `chunkId` | int | yes | 1-based chunk index |
  | `toid` / `grid` | string | yes | Target |
- **Response (`data` after decryption, last chunk):** photo ID in `photoId` (or `clientFileId`); URLs in `normalUrl`, `hdUrl`, `thumbUrl`, `oriUrl`, `url` (any may be missing).
- **Errors:** transport error aborts the send (no fallback for photos on upload failure).
- **Notes:** [I] URL resolution used: `normal = normalUrl | hdUrl | oriUrl | url`; `hd = hdUrl | normalUrl | oriUrl | normal`; `thumb = thumbUrl | normalUrl | hdUrl | normal`.

### Send uploaded photo
- **Request:** `POST {file}/api/message/photo_original/send?zpw_ver=<ver>&zpw_type=30` (1-1) or `POST {file}/api/group/photo_original/send?...` (group)
- **Encoding:** session AES (`zpw_enk`), form body `params=`
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `photoId` | int (string if not numeric) | yes | From upload |
  | `clientId` | int | yes | Same as upload |
  | `desc` | string | yes | Caption (may be `""`) |
  | `width` | int | yes | Image width px |
  | `height` | int | yes | Image height px |
  | `rawUrl` | string | yes | `normal` URL |
  | `hdUrl` | string | yes | `hd` URL |
  | `thumbUrl` | string | yes | `thumb` URL |
  | `oriUrl` | string | yes | `normal` URL |
  | `normalUrl` | string | yes | `normal` URL |
  | `hdSize` | int | yes | Whole-file size in bytes |
  | `zsource` | int | yes | `-1` |
  | `jcp` | string (JSON) | yes | `{"convertible":true}` as a string |
  | `imei` | string | yes | Login IMEI |
  | `toid` / `grid` | string | yes | Target |
  | `visibility` | int | group only | `0` |
- **Response:** `{ msgId, ... }`
- **Errors:** non-zero → send failed.
- **Notes:** used only when upload returned a photo ID **and** a non-empty `normal` URL; otherwise the single-shot fallback below. [I]
  - `width`/`height` are decoded locally from the image; if decoding fails, `800×600` was sent. [I]
  - Thumbnails are **not** generated or uploaded by the client; the server returns `thumbUrl`. [I]

### Single-shot photo (fallback)
- **Request:** `POST {file}/api/message/photo?zpw_ver=<ver>&zpw_type=30&params=<enc>&type=2` (1-1, cmd `11152`) or `POST {file}/api/group/sendpt?...&type=11` (group)
- **Encoding:** session AES (`zpw_enk`) in query; body multipart, field `fileContent`, **whole file**
- **Params:** `clientId`, `desc` (caption), `width`, `height`, `imei`, `toid` | (`grid` + `visibility: 0`). [I]
- **Response:** `{ msgId, ... }`
- **Notes:** [I] path names come from the bundle; this fallback's success was not recorded.

## 3. Videos and files

### Upload file chunk
- **Request:** `POST {file}/api/message/asyncfile/upload?zpw_ver=<ver>&zpw_type=30&params=<enc>&type=2` (1-1) or `POST {file}/api/group/asyncfile/upload?...&type=11` (group)
- **Encoding:** session AES (`zpw_enk`) in query; body multipart, field `chunkContent`
- **Params:** as the photo chunk (§2) **without** `jxl`: `totalChunk`, `fileName`, `clientId`, `totalSize`, `imei`, `isE2EE: 0`, `chunkId`, `toid` | `grid`. [I]
- **Response (last chunk):** file ID in `fileId` (or `clientFileId`); URL in `fileUrl` | `url` | `normalUrl` (may be absent).
- **Errors:** on a transport error the client stopped uploading further chunks and went to the fallback (§3, sharefile).

### Send uploaded file
- **Request:** `POST {file}/api/message/asyncfile/msg?zpw_ver=<ver>&zpw_type=30` (1-1) or `POST {file}/api/group/asyncfile/msg?...` (group)
- **Encoding:** session AES (`zpw_enk`), form body `params=`
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `fileId` | int (string if not numeric) | yes | From upload |
  | `fileUrl` | string | if known | From upload |
  | `checksum` | string | yes | Lower-case hex MD5 of the **whole** file |
  | `checksumSha` | string | yes | `""` (not computed) |
  | `extension` | string | yes | Lower-case extension without dot |
  | `totalSize` | int | yes | Size in bytes |
  | `fileName` | string | yes | Original file name |
  | `clientId` | int | yes | Same as upload |
  | `fType` | int | yes | `2` video, `1` other file |
  | `fileCount` | int | yes | `0` |
  | `fdata` | string (JSON) | yes | `"{}"` |
  | `imei` | string | yes | Login IMEI |
  | `toid` / `grid` | string | yes | Target |
- **Response:** `{ msgId, ... }`; may carry `fileUrl` / `url` / `href`.
- **Errors:** missing `fileId` or non-zero `error_code` → fallback below.
- **Notes:** [I]
  - **No caption** is sent for files/videos (the previous client only displayed it locally).
  - **No video metadata** (duration, width/height, thumbnail) is sent; the correct `fdata` contents for videos are `[U]`.
  - "async" suggests the server finalises the file URL later (possibly over the realtime channel) `[U]`.

### Single-shot file (fallback)
- **Request:** `POST {file}/api/message/sharefile_full?zpw_ver=<ver>&zpw_type=30&params=<enc>&type=2` (1-1, cmd `11146`) or `POST {file}/api/group/sharefile_full?...&type=11` (group)
- **Encoding:** session AES (`zpw_enk`) in query; body multipart, field `fileContent`, **whole file**
- **Params:** only `toid` | `grid` and `clientId` (no `imei`, file name or size). [I] Whether this minimal set is accepted is `[U]`.
- **Response:** `{ msgId, ... }`

## 4. Local display rules (not wire)  [I]

Placeholders the previous client used for its own optimistic row; listed so a rewrite can keep the same look. The visible strings are Vietnamese UI text.

- File size formatting (1024-based): `≥ 1 MiB → "{x:.1} MB"`, `≥ 1 KiB → "{x:.1} KB"`, else `"{n} B"`. Example: 1 468 006 bytes → `1.4 MB`.
- Photo card title: `"{fileName} ({width}×{height} · {size})"`; file/video card title: `"{fileName} ({size})"`.
- Bubble text: the trimmed caption if any, else `[Hình ảnh] {fileName}`, `[Video] {fileName}`, or `[Tập tin] {fileName}`.
- Card source label: `Hình ảnh Zalo`, `Video Zalo`, `Tập tin Zalo`.
- Local message ID: `data.msgId` from the send response, else `out_{clientId}` until the server echo arrives (reconciled by `clientId`, see [../client-rules/message-identity.md](../client-rules/message-identity.md)).
- The just-sent photo was shown from the local bytes immediately (cached under the thumbnail URL's key) to avoid a download round-trip.

## Open questions

- [U] Server-side size limits per kind (photo, video, file) and their error codes.
- [U] Whether the final `photoId` / `fileId` is only in the last chunk's response, and whether chunks may be uploaded in parallel.
- [U] Voice message upload/send endpoint and payload (not implemented).
- [U] Whether GIFs should use a dedicated animated path rather than the photo flow.
- [U] Video metadata (`fdata` contents, duration, thumbnail) expected by `asyncfile/msg`; whether `fileUrl` arrives later via a realtime event.
- [U] Whether captions for files/videos have a wire field.
- [U] Meaning of `zsource: -1`, `jcp`, `isE2EE`, `fileCount`; whether `checksumSha` should be a real SHA hash.
- [U] Whether the minimal `sharefile_full` params and the `message/photo` / `group/sendpt` fallbacks still work.
- [U] Filename encoding expected in the multipart `Content-Disposition` for non-ASCII names.
