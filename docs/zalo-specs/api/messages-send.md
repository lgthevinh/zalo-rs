# Sending messages

How a client sends outgoing chat content to 1-1 and group conversations over REST: plain and rich text, link messages (with preview metadata), quote replies and forwards. Stickers are covered in [../media/stickers.md](../media/stickers.md); file/photo/video attachments in [../media/uploads.md](../media/uploads.md); reactions, delete and recall in [message-actions.md](message-actions.md).

Evidence note: the endpoint paths and field sets below were implemented in the previous client from reading Zalo PC's web bundle (`compact-app-pc.js`, function names such as `sendLinkMessage`, `forwardMultiMessage`) and used against live accounts, but no request/response capture was recorded for most of them. Unless a line says `[O]`, treat it as `[I]`.

## 1. Common request mechanics

All send endpoints in this document share one request shape.  [I]

- **Method:** `POST`, body `Content-Type: application/x-www-form-urlencoded`, body is exactly `params=<value>`.
- **Query string:** the common params only: `zpw_ver=<ver>&zpw_type=30` (see [../auth/session.md](../auth/session.md) for the current `zpw_ver`; the previous client used `694`).
- **`params` value:** URL-encoded base64 of AES-encrypting the JSON body with the session key `zpw_enk` — i.e. session AES, the `encodeAES` path. **Not** ParamCipher v2, and **no `signkey`**. See [../security/payload-encryption.md](../security/payload-encryption.md).  [O] for the `encodeAES`/`zpw_enk` mechanism in general (see [../security/payload-encryption.md](../security/payload-encryption.md)); [I] that every send endpoint uses it.
- **Auth:** session cookie `zpw_sek` (plus `_zlang=vi`), `zpw_sek` also as a header, `Origin`/`Referer` `https://chat.zalo.me`. See [../auth/session.md](../auth/session.md).  [I]
- **Response:** JSON envelope `{error_code, error_message, data}`. When `data` is a non-JSON string it is base64 AES under `zpw_enk`; the decrypted plaintext is itself a full envelope `{error_code, error_message, data}` whose inner `error_code` is authoritative. See [../security/payload-encryption.md](../security/payload-encryption.md).  [I]
- **Success:** `error_code == 0`. The server message ID is `data.msgId` (string or number — normalise to string).  [I]
- **Errors:** any non-zero `error_code` is a failure. `600` (`zpw_sek not found or invalid`) and `-102` are treated as "session expired → re-login". See [../client-rules/errors.md](../client-rules/errors.md).  [O] for 600 on unauthenticated requests ([../client-rules/errors.md](../client-rules/errors.md)); [I] for `-102`.

### 1.1 Hosts

| Kind | Service key | Default host |
|---|---|---|
| 1-1 sends (`/api/message/*`) | `chat` | `https://tt-chat3-wpa.api.zaloapp.com` |
| Group sends (`/api/group/*`) | `group` | `https://tt-group-wpa.zaloapp.com` |

- [O] Both default hosts answered `error_code 0` in a live QR session (service map v3 snapshot 2026-10-21).
- [I] **Do not** send REST message requests to the message-plane long-poll host from `zpw_chts_new` (`p<N>-msg-chat.zaloapp.com`). The previous client, whenever its `chat` host was empty, contained `-msg-chat.`, or contained `tt-chat2-wpa`, replaced it with the `tt-chat3-wpa` default above. The reason for avoiding `tt-chat2-wpa` is not recorded `[U]`.

### 1.2 Common body fields

| Field | Type | Meaning |
|---|---|---|
| `clientId` | integer (ms-shaped) | Client-generated message ID. Must be unique per send; the server echoes it back as the message's `cliMsgId` (see [message-identity.md](../client-rules/message-identity.md)). [I] |
| `imei` | string | The device IMEI/client ID used at login (`<imei>`). Present on most but not all endpoints — see each table. [I] |
| `toid` / `toId` | string | 1-1 peer user ID (`<uid>`). Note the casing differs by endpoint. [I] |
| `grid` | string | Group ID, bare numeric form (no local `g`/`group_` prefix). [I] |
| `visibility` | int, `0` | Sent on group endpoints only. Meaning unknown `[U]`. |
| `ttl` | int, `0` | Self-destruct timer; `0` = never. Sent on link and quote, **not** on plain text. Units and non-zero behaviour unknown `[U]`. |
| `language` | string, `"vi"` | Sent on link (both) and group quote. [I] |
| `zsource` | int | `101` on quote requests. Meaning unknown `[U]`. |

### 1.3 `clientId` allocation rule  [I]

`clientId` = current Unix time in milliseconds, but strictly greater than the previous value issued by this process: `id = max(now_ms, last_id + 1)`. This prevents two sends in the same millisecond sharing an ID. Allocate **once per logical send** and reuse the same value across fallbacks (e.g. link → plain text fallback), so the server echo can be reconciled with the optimistic local row. See [message-identity.md](../client-rules/message-identity.md) and [../client-rules/message-model.md](../client-rules/message-model.md).

## 2. Text messages

### Send 1-1 text
- **Request:** `POST {chat}/api/message/sms?zpw_ver=<ver>&zpw_type=30`  (default host: `https://tt-chat3-wpa.api.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`), form body `params=`; no signkey
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `toid` | string | yes | Peer user ID |
  | `message` | string | yes | Message text (after rich-text extraction, §3) |
  | `clientId` | int | yes | §1.3 |
  | `imei` | string | yes | Login IMEI |
  | `textProperties` | string (JSON) | no | Only when the text has styles (§3) |
- **Response (`data` after decryption):** `{ msgId, ... }`
- **Errors:** see §1
- **Notes:** no `ttl`, no `visibility`, no `language` sent. [I] Endpoint path is listed in the live host table in [../auth/service-discovery.md](../auth/service-discovery.md); the full request was not captured. [I]

### Send group text
- **Request:** `POST {group}/api/group/sendmsg?zpw_ver=<ver>&zpw_type=30`  (default host: `https://tt-group-wpa.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`); no signkey
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `grid` | string | yes | Bare group ID |
  | `message` | string | yes | Message text (after §3) |
  | `clientId` | int | yes | §1.3 |
  | `imei` | string | yes | Login IMEI |
  | `visibility` | int | yes | Always `0` |
  | `textProperties` | string (JSON) | no | Only when styled |
- **Response:** `{ msgId, ... }`
- **Errors:** see §1
- **Notes:** mentions (`@name`) are **not** implemented for plain group text; no `mentionInfo` field is sent here. The wire format for mentions is `[U]` (see Open questions). [I]

### 1-1 vs group summary  [I]

| Aspect | 1-1 | Group |
|---|---|---|
| Host key | `chat` | `group` |
| Path prefix | `/api/message/` | `/api/group/` |
| Target field | `toid` (link: `toId`) | `grid` |
| `visibility` | absent | `0` |
| Text path | `sms` | `sendmsg` |
| Link path | `link` | `sendlink` |
| Quote path | `quote` | `quote` |

## 3. Rich text (`textProperties`)

Zalo PC sends formatting as a separate JSON string alongside the plain `message`.  [I] (format taken from the PC bundle; no send capture recorded.)

```json
{"styles":[{"start":0,"len":4,"st":"b"},{"start":5,"len":3,"st":"b,f_18"}],"ver":0}
```

- `textProperties` is sent as a **JSON-encoded string** (a string field containing JSON), not a nested object. [I]
- `start` / `len` are offsets in **UTF-16 code units** into the final `message` text (after markup is removed). [I]
- `st` is a comma-separated tag list. Known tags: [I]

  | Tag | Meaning |
  |---|---|
  | `b` | bold |
  | `i` | italic |
  | `u` | underline |
  | `s` | strikethrough |
  | `f_18` | large font (heading); `f_16` also used for a smaller heading |
  | `lst_1` | bulleted list line |
  | `lst_2` | numbered list line |

  Colour tags and other sizes may exist in the PC client `[U]`.
- `ver` is `0`. [I]
- If there are no styles, omit `textProperties` entirely and send the raw text unchanged. [I]

### 3.1 Markdown → `textProperties` conversion used by the previous client  [I]

This is a client convenience, not a Zalo rule; a re-implementation may use any editor model as long as it produces the wire shape above.

```
for each line (split on "\n", re-joined with "\n"):
    line prefix:  "### "      -> line tags [b, f_16]
                  "## " / "# " -> line tags [b, f_18]
                  "- " / "* "  -> line tags [lst_1]
                  "1. "        -> line tags [lst_2]
    (prefix is stripped from the output text)
    scan the rest left to right; at each position try delimiters in order
        "`" (no tag), "**" (b), "__" (u), "~~" (s), "*" (i)
      if the delimiter has a matching closer later on the same line with non-empty inner text:
         emit inner text, add style {start, len, st} (UTF-16 units) unless tag is empty
      else emit one character
    if the line had line tags and non-empty output: add style covering the whole line output
```

Spans do not nest (the first matching delimiter wins; inner markup is emitted literally).

### 3.2 Incoming rich text (for symmetry)  [I]

Messages from official clients arrive with content `{"action":"rtf","title":<text>,"params":"<JSON string with styles>"}`; `params.styles` has the same `{start,len,st}` shape. Any `f_*` tag was rendered as bold. Details belong to [../client-rules/message-model.md](../client-rules/message-model.md).

## 4. Link messages

A link message is a text message with a preview card (title, description, thumbnail, source). The **client** supplies all preview fields in the send request; the server does not appear to fetch the page itself at send time. [I]

### 4.1 Client-side flow  [I]

1. On send, find the first URL in the trimmed text: first occurrence of `https://`, else `http://`; the URL ends at the first whitespace; strip trailing `)`, `]`, `,`, `;`. Ignore if the result is shorter than 10 characters. If no URL → send as plain text (§2).
2. Fetch preview metadata with **Parse link** (§4.2). If that request fails entirely, use `{url, title = url, desc = "", thumb = "", src = "Liên kết"}`.
3. If any of title/desc/thumb is still empty, fill the gaps by scraping the page's OpenGraph tags (§4.3).
4. If title is still empty, use the URL's host (domain) as title.
5. Send with **Send link** (§4.4) using one `clientId`. If it fails (transport error or non-zero `error_code`), fall back to a plain text send (§2) of the same message string with the **same** `clientId`.

Note: the link path sends the trimmed text as-is — it does **not** run the rich-text conversion (§3), so markup characters are sent literally in link messages. Only the plain-text fallback converts them. [I]

### Parse link (server-side preview)
- **Request:** `GET {file}/api/message/parselink?zpw_ver=<ver>&zpw_type=30&params=<enc>`  (default host: `https://tt-files-wpa.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`), `params` in the query string; no signkey
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `link` | string | yes | The URL |
  | `version` | int | yes | `1` |
  | `imei` | string | yes | Login IMEI |
- **Response (`data` after decryption):** fields `title`, `desc`, `thumb`, `src` — either directly under `data` or nested under `data.data` (accept both). [I]
- **Errors:** treat any failure as "no server metadata" and continue with scraping.
- **Notes:** [I] Zalo cmd `11285` per bundle reference.
  - Discard `title` / `desc` if empty or equal to the URL itself.
  - Discard `thumb` if it is a Zalo placeholder: empty, or contains `feed_thumb_link`, or `im_parselink`, or the base64 string `aHR0cHM6Ly9yZXMtemFsby56YWRuLnZuL3VwbG9hZC9tZWRpYS8yMDE5LzEwLzE1L2ZlZWRfdGh1bWJfbGlua19fMV9fMTU3MTEzMzEyMjc3OF85Mzc4MC5wbmc=` (a base64-wrapped URL of the generic link thumbnail). [I]
  - `src` (source/site label) is taken when non-empty; otherwise default to the URL's host.
  - The previous client's fallback default host for this one call was `https://tt-files-wpa.chat.zalo.me`, which differs from the `file` default elsewhere (see Open questions).

### 4.3 OpenGraph scrape (client-only, not a Zalo API)  [I]

Plain HTTP GET of the target URL, no Zalo cookies:

- `User-Agent: Mozilla/5.0 (compatible; Discordbot/2.0; +https://discordapp.com)` (bot UA so sites serve OG tags), `Accept-Language: vi-VN,vi;q=0.9,en-US;q=0.8`.
- Timeout ≈ 3.8 s, at most 5 redirects; non-2xx → give up. Only the first 128 KiB of the body is parsed.
- From `<meta>` tags (key from `property` or `name`, value from `content`, HTML entities unescaped), first non-empty match wins:
  - title: `og:title`, `twitter:title`, `lark:url:video_title`; else `<title>` text
  - desc: `og:description`, `twitter:description`, `description`
  - thumb: `og:image`, `twitter:image`, `lark:url:video_cover_image_url` (only if value starts with `http`)
  - src: `og:site_name`, else the URL host
- Scraped values only fill fields the server left empty; scraped `src` replaces the server value only if that was empty or equal to the bare host.

### Send link (1-1)
- **Request:** `POST {chat}/api/message/link?zpw_ver=<ver>&zpw_type=30`  (default host: `https://tt-chat3-wpa.api.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`); no signkey
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `toId` | string | yes | Peer user ID — **capital `I`**, unlike `toid` elsewhere |
  | `msg` | string | yes | Full message text (trimmed); if empty, the URL |
  | `href` | string | yes | The URL |
  | `src` | string | yes | Source label (site name / host) |
  | `title` | string | yes | Preview title; if empty, the URL |
  | `desc` | string | yes | Preview description (may be `""`) |
  | `thumb` | string | yes | Preview image URL (may be `""`) |
  | `type` | int | yes | `0` |
  | `media` | string (JSON) | yes | `{"type":0,"count":0,"mediaTitle":"","artist":"","streamUrl":"","stream_icon":""}` serialised as a string |
  | `clientId` | int | yes | §1.3 |
  | `mentionInfo` | string | yes | `""` |
  | `language` | string | yes | `"vi"` |
  | `ttl` | int | yes | `0` |
  | `imei` | string | yes | Login IMEI |
- **Response:** `{ msgId, ... }`
- **Errors:** non-zero → fall back to plain text (§4.1 step 5)
- **Notes:** Zalo cmd `11286` / `sendLinkMessage` per bundle reference. [I]

### Send link (group)
- **Request:** `POST {group}/api/group/sendlink?zpw_ver=<ver>&zpw_type=30`  (default host: `https://tt-group-wpa.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`); no signkey
- **Params:** same as 1-1 except: `grid` (bare group ID) instead of `toId`, and `mentionInfo` is `"[]"` instead of `""`. No `visibility`. [I]
- **Response:** `{ msgId, ... }`
- **Notes:** Zalo cmd `11193` per bundle reference. [I]

## 5. Quote replies

### Send quote (1-1)
- **Request:** `POST {chat}/api/message/quote?zpw_ver=<ver>&zpw_type=30`  (default host: `https://tt-chat3-wpa.api.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`); no signkey
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `toid` | string | yes | Peer user ID |
  | `message` | string | yes | Reply text (after §3) |
  | `clientId` | int | yes | §1.3 |
  | `qmsgOwner` | string | yes | User ID of the quoted message's sender (see rules) |
  | `qmsgId` | int | yes | Server `globalMsgId` of the quoted message, as a number |
  | `qmsgCliId` | int | yes | Client message ID of the quoted message (see rules) |
  | `qmsgType` | int | yes | Always `1` in the previous client |
  | `qmsgTs` | int | yes | Quoted message timestamp (ms) |
  | `qmsg` | string | yes | Quoted message's display text |
  | `imei` | string | yes | Login IMEI |
  | `ttl` | int | yes | `0` |
  | `zsource` | int | yes | `101` |
  | `textProperties` | string (JSON) | no | Only when styled |
- **Response:** `{ msgId, ... }`
- **Errors:** non-zero → fall back to plain text of the reply (same `clientId`); the quote link is lost.
- **Notes:** [I] endpoint listed in the host table of [../auth/service-discovery.md](../auth/service-discovery.md).

### Send quote (group)
- **Request:** `POST {group}/api/group/quote?zpw_ver=<ver>&zpw_type=30`  (default host: `https://tt-group-wpa.zaloapp.com`)
- **Encoding:** session AES (`zpw_enk`); no signkey
- **Params:** as 1-1, but `grid` instead of `toid`, plus `language: "vi"` and `visibility: 0`. [I]
- **Response:** `{ msgId, ... }`

### 5.1 Quote field rules used  [I]

- **Quote requires a server ID.** If the quoted message has no server `globalMsgId` yet (still a local/pending row), skip the quote endpoint and send plain text.
- `qmsgOwner`: own UID if the quoted message is outgoing; else the sender's numeric UID if known; else (1-1) the peer ID; else (group, sender UID unknown) `"0"`. Whether the server accepts `"0"` is `[U]`.
- `qmsgId`: the quoted message's server ID parsed as an integer.
- `qmsgTs`: the quoted message's timestamp in ms.
- `qmsgCliId`: the previous client sent **the quoted message's timestamp** here, not its real `cliMsgId` (it did not keep `cliMsgId` handy). Correctness of this substitution is `[U]` — a re-implementation should send the real `cliMsgId` if it has it, and verify.
- `qmsgType: 1` was sent for every quoted message regardless of its kind (text, photo, sticker). Correct values for non-text quotes and any `qmsgAttach`-style field are `[U]`.
- Incoming quoted messages carry a `quote` object with `cliMsgId` / `globalMsgId` / `msgId`, a sender (`fromD`, `ownerId`, …) and text in `msg`. See [../client-rules/message-model.md](../client-rules/message-model.md).

## 6. Forward

The previous client did **not** call a server-side forward endpoint. [I] It re-sent the content as a new message to the target conversation with a fresh `clientId`:

- If the source message is a sticker (sticker ID > 0) → send the same sticker (`stickerId`, `cateId`, type default `7`) via the sticker endpoint — see [../media/stickers.md](../media/stickers.md).
- Otherwise → send the source message's display text via §2 (1-1 or group per target).

Consequences: photos/files/links are forwarded as their text placeholder only; no "forwarded" marker is set. Zalo PC's bundle has a `forwardMultiMessage` function; its endpoint and payload were not extracted `[U]`.

## Open questions

- [U] Server-side forward endpoint (`forwardMultiMessage`): path, params, and whether it preserves attachments.
- [U] Mentions: wire shape of `mentionInfo` for group text (`"[]"` vs `""` is all that is known; positions/uids format unknown). Plain `sendmsg` sent no mention field at all.
- [U] Meaning of `visibility`, `zsource` (`101`), and non-zero `ttl` (self-destruct units).
- [U] Correct `qmsgCliId` (real `cliMsgId` vs timestamp as sent) and `qmsgType` values for non-text quoted messages; whether `qmsgOwner: "0"` is accepted.
- [U] Why `tt-chat2-wpa` is avoided as the `chat` host.
- [U] Parse-link host: the previous client used `file` (`https://tt-files-wpa.zaloapp.com`) as default for uploads but `https://tt-files-wpa.chat.zalo.me` as the parse-link fallback — which one the server map actually serves is not recorded.
- [U] Whether rich-text `textProperties` is accepted on `link` requests (it was never sent there).
- [U] Additional `st` tags (colours, other font sizes) used by Zalo PC.
- [U] Whether `sms` requires `ttl`/`language` on current server versions (not sent; no capture recorded).
