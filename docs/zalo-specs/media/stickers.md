# Stickers

How Zalo identifies stickers, how a client lists the sticker store, searches stickers by
keyword, sends a sticker, and fetches sticker images. Recorded from the previous client
(Zalo PC bundle `compact-app-pc.*.js`, web session flavour, 2026-10).

Evidence tags: `[O]` observed / live-verified, `[I]` implemented in the previous client without
recorded live evidence, `[U]` unknown.

Related: [../security/payload-encryption.md](../security/payload-encryption.md) (session AES),
[../auth/service-discovery.md](../auth/service-discovery.md) (service map),
[../api/messages-send.md](../api/messages-send.md) (client message IDs, send echo),
[../client-rules/message-model.md](../client-rules/message-model.md) (`msgType` codes),
[../media/uploads.md](uploads.md) (photo/GIF upload), [downloads.md](downloads.md) (CDN fetches).

---

## 1. Sticker identity

A sticker is identified by three numbers: [I]

| Field | Wire names | Meaning |
|---|---|---|
| sticker id | `stickerId`, `id`, `sticker_id` | Unique sticker number. `0` means "no sticker". |
| category id | `cateId`, `catId`, `cate_id` | The pack ("category") the sticker belongs to. `0` = the built-in default pack. |
| type | `type` | Sticker kind. The previous client defaulted to `7` everywhere it was missing; a fixture also used `1`. Meaning of the values is `[U]`. |

- The same concept is spelled differently by different endpoints (`cateId` vs `catId` vs
  `cate_id`; `id` vs `stickerId` vs `sticker_id`). A parser must accept all of them. [I]
- Numeric fields may arrive as JSON numbers or as numeric strings; accept both. [I]
- In a received sticker message, the `content` field may be a JSON **string** that itself
  encodes an object such as `{"catId":<cid>,"id":<sid>,"type":<t>}`; decode it before reading. [I]
  (fixture-based test, not a capture)
- An object counts as a sticker only if it has a sticker id **and** at least one of
  `stickerId`, `catId`, `cateId` (a bare `id` alone is too generic). [I]
- Message type: string `chat.sticker`; numeric codes are contradictory across sources — see
  [../client-rules/message-model.md](../client-rules/message-model.md) and Open questions. [I]

### 1.1 Default pack (category 0)

- Category `0` with sticker ids `1..30` is Zalo's built-in default ("Củ hành" / onion) pack. It
  is not fetched from the server; the ids are hardcoded and their images come from a static
  host (§6). [I]

---

## 2. Hosts

| Purpose | Service key | Default host |
|---|---|---|
| Sticker API (catalog, detail, suggest, search) | `sticker` | `https://tt-sticker-wpa.chat.zalo.me` [O — listed in the live service map table] |
| Send sticker, 1-1 | `chat` | `https://tt-chat3-wpa.api.zaloapp.com` |
| Send sticker, group | `group` | `https://tt-group-wpa.zaloapp.com` |
| Store catalog JSON (CDN) | — (URL comes from the catalog response, §3.1) | fallback `https://zalo-api.zadn.vn/api/emoticon/pcweb/getAllCate` |
| Sticker image | — (fixed) | `https://api.zaloapp.com/api/emoticon/sticker/webpc` [I] |
| Default-pack image | — (fixed) | `https://stc-chat.zdn.vn/images/stickers/default/thumb/` [I] |

- The PC bundle's built-in default for the sticker service is `https://sticker-wpa.{CONFIG_DOMAIN}`
  (see [../auth/service-discovery.md](../auth/service-discovery.md)); the live web service map returned the
  `tt-sticker-wpa.chat.zalo.me` value above. Always prefer the server's service map. [O]
- Suggest-keywords had a hardcoded secondary host `https://tt-sticker-wpa.zaloapp.com`, used
  only when the request to the primary sticker host failed at transport level. [I]

---

## 3. Common request rules for the sticker API

All sticker-service calls in §3–§4 are authenticated `GET`s: [I]

```
GET {sticker}/api/message/sticker/<path>?zpw_ver=<ver>&zpw_type=<type>&params=<P>
```

- `P` = URL-encode( base64( session-AES-encrypt( JSON.stringify(paramsObject) ) ) ) using the
  session key `zpw_enk` — see [../security/payload-encryption.md](../security/payload-encryption.md).
  No `signkey` was added for these calls. [I]
- Session cookie (`zpw_sek`) and the normal Origin/Referer (`https://chat.zalo.me`) headers are
  sent as for every API call ([../auth/session.md](../auth/session.md)). [I]
- Response envelope: `{error_code, error_message, data}`. When `data` is a non-empty string
  that does not start with `{`, it is session-AES ciphertext; the decrypted plaintext is itself
  a **full envelope** (with its own `error_code`) and must replace the outer one. [I]
- Several handlers tolerate either `data.<field>` or a top-level `<field>` after decryption
  (the exact nesting was not pinned down). [U]

### 3.1 List store categories (catalog)

Two-step: an API call returns a CDN URL, the CDN serves the full catalog. Bundle name
`getStickerCatsV2` → `getAllCate`. [I]

**Step 1**
- **Request:** `GET {sticker}/api/message/sticker/category/list/v2`  (default host: `https://tt-sticker-wpa.chat.zalo.me`)
- **Encoding:** session AES (`zpw_enk`), no signkey. **Quirk:** the encrypted plaintext is the
  **empty string** `""`, not `{}`. [I]
- **Params (plaintext, before encryption):** none (empty string).
- **Response (`data` after decryption):**
  | Field | Type | Meaning |
  |---|---|---|
  | `dataUrl` | string | URL of the full catalog JSON on a CDN. May be `http://`; upgrade to `https://`. |
  | `exCateIds` | number[] | Category ids to **exclude** from display. |
- **Errors:** non-zero `error_code` → treat as failure. Specific codes `[U]`.
- **Notes:** if `dataUrl` is missing, the previous client fell back to
  `https://zalo-api.zadn.vn/api/emoticon/pcweb/getAllCate`. [I]

**Step 2**
- **Request:** `GET <dataUrl>` (plain, unauthenticated CDN fetch). [I]
- **Encoding:** plain. The body may be **gzip-compressed without a matching
  `Content-Encoding`**: check the magic bytes `1F 8B` and gunzip manually. [I]
- **Response:** a JSON array of categories:
  | Field | Type | Meaning |
  |---|---|---|
  | `id` | number | Category id (`cateId`). |
  | `name` | string | Display name. |
  | `totalImage` | number | Sticker count (client defaulted to 16 when absent). |
  | `iconUrl` / `thumbImg` | string | Pack icon URL (prefer `iconUrl`); upgrade `http://` → `https://`. |
- **Client filtering (policy, not server rule):** drop `id == 0`, ids in `exCateIds`, empty
  names, and names containing "test" (case-insensitive). Sort by `id` descending so newer packs
  come first (larger id ≈ newer pack). About 390 packs were returned at the time. [I]

### 3.2 Get stickers in a category

- **Request:** `GET {sticker}/api/message/sticker/category/sticker_detail`  (default host: `https://tt-sticker-wpa.chat.zalo.me`)
  Bundle name `getStickerCategoryDetail` (bundle cmd label 11284). [I]
- **Encoding:** session AES (`zpw_enk`).
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `cid` | **string** | yes | Category id, sent as a decimal string. |
- **Response (`data` after decryption):** array of sticker objects (or the decrypted value is the
  array itself): `id`/`stickerId`, `cateId` (defaults to the requested `cid`), `type` (default 7). [I]
- **Errors:** `[U]`.
- **Notes:** never call with `cid = 0`; the default pack is local (§1.1). An earlier host table
  lists the path as `/api/message/sticker/sticker_detail` — the implemented path includes
  `/category/`; see Open questions.

### 3.3 Offline pack fallback (data, not protocol)

The previous client hardcoded a few pack → sticker-id ranges so the picker worked before the
catalog loaded. These are observed catalog data and may go stale: [I]

| cateId | Pack | Sticker ids |
|---|---|---|
| 61900 | Zapy Dô Tri | 98930–98941 |
| 12815 | Bé Hai | 101001–101030 |
| 12748 | Cò Lõ Đi Làm | 99978–99997 |
| 12705 | Mèo Meon | 99278–99293 |
| 12718 | Gấu Cay | 99486–99501 |
| 12045 | Zameo | 47614–47624, 47626–47633 |
| 11150 | Bé Dúi | 34107–34110, 34827–34837 |
| 10186 | Đậu Đỏ | 18974–18995 |
| 10177 | Heo Luci | 18847–18871 |
| 10117 | Chó Toto | 17745–17764 |

All with `type = 7`. Prefer `sticker_detail` whenever possible.

---

## 4. Keyword suggestion and search

Zalo PC shows a sticker suggestion bar while the user types. Three endpoints are involved.

### 4.1 Get suggestion trigger keywords

- **Request:** `GET {sticker}/api/message/sticker/suggest/keywords`  (default host: `https://tt-sticker-wpa.chat.zalo.me`; transport-failure fallback `https://tt-sticker-wpa.zaloapp.com`)
  Bundle name `getStickerListKeyWords` (cmd label 11740). [I]
- **Encoding:** session AES (`zpw_enk`).
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `imei` | string | yes | Device id `<imei>`. |
- **Response (`data` after decryption):** `{ "keywords": [string, ...] }`. [I]
- **Notes:** entries starting with `zkwd_` are internal tokens, not user text — skip them. Store
  each keyword normalised (§4.4) **and** diacritic-stripped so both accented and unaccented typing
  matches. [I]

### 4.2 Suggest stickers for a keyword

- **Request:** `GET {sticker}/api/message/sticker/suggest/stickers`  (default host: `https://tt-sticker-wpa.chat.zalo.me`)
  Bundle name `getStickerSuggestion` (cmd label 11741). [I]
- **Encoding:** session AES (`zpw_enk`).
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `keyword` | string | yes | Query text. |
  | `gif` | number | yes? | `1` in the previous client. Meaning `[U]` (likely "include animated"). |
  | `guggy` | number | yes? | `0`. Meaning `[U]` (Guggy is a third-party GIF service). |
  | `imei` | string | yes | `<imei>`. |
- **Response (`data` after decryption):** `{ "sugg_sticker": [ {sticker_id|id, cate_id|cateId, type}, ... ] }`. [I]

### 4.3 Search stickers

- **Request:** `GET {sticker}/api/message/sticker/search`  (default host: `https://tt-sticker-wpa.chat.zalo.me`)
  Bundle name `searchSticker` (cmd label 11747). [I]
- **Encoding:** session AES (`zpw_enk`).
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `keyword` | string | yes | Query text. |
  | `limit` | number | ? | `30` in the previous client. |
  | `srcType` | number | ? | `0`. Meaning `[U]`. |
  | `imei` | string | yes | `<imei>`. |
- **Response (`data` after decryption):** array of `{sticker_id|id, cate_id|cateId, type}`
  (`data` itself is the array, or the decrypted value is). [I]

### 4.4 Keyword normalisation (client algorithm)

Server-side indexing appears to use **accented** Vietnamese keywords (e.g. `nhớ`, `khóc`), while
users often type unaccented text, slang or English. The previous client compensated with the
following pipeline. It is a recall heuristic, not a server rule. [I]

```
normalize(s):
    replace U+00A0 (NBSP) with ' '
    split on Unicode whitespace, drop empties, join with single ' '
    lowercase

strip_vn(s):                      # mirrors bundle fn simpleStripVietnamese
    for each char c in lowercase(s):
        à á ạ ả ã â ầ ấ ậ ẩ ẫ ă ằ ắ ặ ẳ ẵ → a
        è é ẹ ẻ ẽ ê ề ế ệ ể ễ           → e
        ì í ị ỉ ĩ                        → i
        ò ó ọ ỏ õ ô ồ ố ộ ổ ỗ ơ ờ ớ ợ ở ỡ → o
        ù ú ụ ủ ũ ư ừ ứ ự ử ữ           → u
        ỳ ý ỵ ỷ ỹ                        → y
        đ                                → d
        U+00A0                           → ' '
        anything else unchanged
    collapse whitespace as in normalize()

queries(keyword):
    n = normalize(keyword);  if n == "" → []
    q = [n]
    s = strip_vn(n);  if s != "" and s != n → append s
    a = SYNONYMS[s]   # lookup by stripped form
    if a: append a (if new); append strip_vn(a) (if new)
    return first 4 of q (deduplicated, order kept)
```

Notes:
- Only **precomposed** (NFC) code points are mapped. Decomposed input (base letter + combining
  mark, e.g. from some IMEs) is not folded; normalise to NFC first. [I — gap in old client]
- `SYNONYMS` maps unaccented Vietnamese / slang / English to one accented Zalo keyword. The
  previous client's table (stripped key → canonical):

| Canonical | Stripped inputs |
|---|---|
| nhớ | nho, nho em, nho anh, nho qua, miss, miss u, miss you |
| khóc | khoc, huhu, huhuhu, hic, cry |
| buồn | buon, buon qua, sad, tui than |
| vui | vui, vui ve, vui qua, happy |
| cười | cuoi, mac cuoi, buon cuoi |
| yêu | yeu, iu, yeu em, yeu anh, iu em, iu anh, love, love u, love you, tha tim, tim |
| thương | thuong, thuong em, thuong anh, thuong qua |
| giận | gian, gian doi, doi hon, tuc, buc, buc minh, cay, angry |
| mệt | met, met qua, met moi, duoi, duoi qua, tired |
| đói | doi, doi bung, doi qua, hungry |
| ăn cơm | an, an com, di an |
| cà phê | ca phe, cf, cafe, coffee |
| trà sữa | tra sua, trasua |
| nhậu | nhau, di nhau, bia, dzoo, dzo |
| ngủ | ngu, di ngu, buon ngu, ngu ngon, ngu ngon nhe, ngu ngon nha, good night, goodnight, gn, g9 |
| xin chào | chao, xin chao, chao ban, chao anh, chao chi, chao em, he lo, helo |
| cảm ơn | cam on, cam on ban, cam on anh, cam on chi, cam on em, cam on nhieu, thank, thanks, thank you, tks, tkss, thank kiu |
| xin lỗi | xin loi, xin loi nha, xin loi nhe, sorry, sr, so ry |
| tạm biệt | tam biet, tam biet nhe, bye, byebye, good bye, goodbye, bibi, pp |
| chúc mừng | chuc mung, happy birthday, hpbd, sinh nhat, cmsn |
| tết | tet, chuc mung nam moi, nam moi |
| cố lên | co len, co gang, fighting |
| tuyệt vời | tuyet, tuyet voi, xuat sac, dinh, dinh qua, chuan, duoc |
| ngại | ngai, ngai qua, ngai nhi, xau ho |
| sợ | so, so qua, scared |
| lạnh | lanh, lanh qua, lanh khong, ret, ret qua |
| nóng | nong, nong qua, nong buc |
| tiền | tien, luong |
| hết tiền | het tien, ngheo, ngheo qua, vi rong |
| lạy | quy, lay, xin, van xin |
| bận | ban, ban qua, busy |
| chán | chan, chan qua, chan doi, boring |
| chờ chút | cho, cho chut, cho xiu, doi chut, doi xiu, wait |
| đi chơi | di choi, choi, dao pho |
| làm | di lam, lam viec, deadline, code |
| ngạc nhiên | hong, hong chuyen, ngac nhien |

  Note the collision: stripped `doi` maps to `đói` (hungry) although it could be `đợi`/`đổi`. [I]

### 4.5 Search orchestration and ranking (client algorithm)

```
search(keyword):
    kw = normalize(keyword); if empty → []
    Q  = queries(kw)
    for each q in Q (concurrently): call suggest/stickers(q) and search(q)
    each hit scores 3 if q is Q[0] (the user's own text) else 2
    merge by sticker id: sum scores; keep first non-zero cate id seen
    if no hits and kw has 2..5 words:
        try queries(last two words) [only when ≥3 words], then queries(last word) [if ≥2 chars];
        stop at first non-empty result
    LEGACY = {0, 50, 10788, 11190, 11984, 11998, 12002, 12003, 12024, 12658}
    if any hit has cate ≥ 10000 and cate ∉ LEGACY:
        keep only hits with cate ≥ 10000 and cate ∉ LEGACY
    sort by score desc, then cate id desc
    first pass: take at most 2 per cate; then fill from the rest; cap 24 results
```

- `LEGACY` lists round-emoji / photo-meme packs the previous client considered low quality; it
  claimed to mirror Zalo PC's `pointCate` ranking. Not verified. [I]
- Composer UI debounced alphabetic typing by ~145 ms so Vietnamese Telex/VNI composition
  finishes before the request fires. [I]

---

## 5. Send a sticker

Bundle name `sendSticker`. [I]

### Send sticker (1-1)
- **Request:** `POST {chat}/api/message/sticker?zpw_ver=<ver>&zpw_type=<type>`  (default host: `https://tt-chat3-wpa.api.zaloapp.com`) — bundle cmd label 11154
- **Encoding:** session AES (`zpw_enk`); body `application/x-www-form-urlencoded`: `params=<P>`. No signkey. [I]
- **Params (plaintext, before encryption):**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `toid` | string | yes | Recipient user id `<uid>`. |
  | `stickerId` | number | yes | Sticker id. |
  | `cateId` | number | yes | Category id. |
  | `type` | number | yes | Sticker type (usually 7). |
  | `clientId` | number | yes | Client message id (ms timestamp-like), echoed back for reconciliation — see [../api/messages-send.md](../api/messages-send.md). |
  | `imei` | string | yes | `<imei>`. |
  | `zsource` | number | yes | Constant `101`. |
- **Response (`data` after decryption):** standard send result (message id etc.) — see [../api/messages-send.md](../api/messages-send.md).
- **Errors:** any non-zero `error_code` → failure, surface `error_message`. Specific codes `[U]`;
  general ones in [../client-rules/errors.md](../client-rules/errors.md).
- **Notes:** host quirk — if the resolved `chat` host is empty, contains `-msg-chat.` (a
  long-poll message-plane host such as the `zpw_chts_new` value) or contains `tt-chat2-wpa`, the
  previous client forced `https://tt-chat3-wpa.api.zaloapp.com` instead. Why `tt-chat2-wpa` was
  rejected is not recorded. [I]

### Send sticker (group)
- **Request:** `POST {group}/api/group/sticker?zpw_ver=<ver>&zpw_type=<type>`  (default host: `https://tt-group-wpa.zaloapp.com`) — bundle cmd label 11165
- **Encoding:** session AES (`zpw_enk`); form body `params=<P>`. [I]
- **Params (plaintext, before encryption):** same as 1-1 but `grid` (group id `<gid>`, string)
  instead of `toid`.
- **Response / Errors:** as 1-1.
- **Notes:** the group id must be the bare numeric id; the previous client stripped its own local
  prefixes (`group_`, `g`, `G`) before sending. [I]

---

## 6. Sticker images

### 6.1 URLs

```
if cateId == 0 and 1 <= stickerId <= 30:
    https://stc-chat.zdn.vn/images/stickers/default/thumb/<stickerId>.png
else:
    https://api.zaloapp.com/api/emoticon/sticker/webpc?eid=<stickerId>&size=130
```
Source: PC bundle (two sites in `compact-app-pc.*.js`). [I]

- Only `eid` (sticker id) is needed; `cateId` is not part of the URL. [I]
- `size=130` is the only value used; other sizes `[U]`.
- Requests were sent with the normal session HTTP client (cookies, `Origin`/`Referer`
  `https://chat.zalo.me`, Zalo PC user agent). Whether the endpoint needs any of them is `[U]`.
- Pack icons come from the catalog's `iconUrl`/`thumbImg` (§3.1).

### 6.2 Formats and animation

- The previous client treated every sticker response as a **single static image**, decoded by
  content sniffing (not by extension) and cached under a `.png` name regardless of real format.
  This worked, so the `webpc` endpoint returns a decodable raster image (PNG or WebP) at least as
  a first frame. [I]
- **Animated stickers, sprite sheets and WebP animation were not implemented**; the real format
  of animated Zalo stickers (sprite strip vs animated WebP vs other) and the URL that serves the
  animated variant are unknown. [U]
- Zalo has a separate GIF message type (`chat.gif`); see
  [../client-rules/message-model.md](../client-rules/message-model.md). Not handled for stickers. [I]

### 6.3 Client-local GIF "stickers" (not a Zalo feature)

The previous client shipped its own animated pack ("Mèo Ami Bụng Bự", 15 GIFs). These are **not**
Zalo stickers and must not be sent via `sendSticker`: [I]
- Local ids: category `99901`, stickers `999001..999015` — fake, unknown to Zalo.
- Sending uploads the GIF as a normal photo/attachment ([uploads.md](uploads.md)) with the
  caption `#ami_fat_cat_NN` (two-digit index).
- On receive, any message whose text, description, title or URL contains `ami_fat_cat_NN`
  (`NN` in 01..15) was re-rendered locally as that sticker. Other Zalo clients just see a GIF
  photo with the caption. Keep or drop as a product decision.

Likewise, a user-defined `custom_stickers.json` let users add packs as `{cate_id, name,
sticker_ids[], sticker_type}`; those ids must be real Zalo ids to be sendable. [I]

### 6.4 Caching

- Cache key: `stk_<cateId>_<stickerId>` (cateId `0` when unknown). [I]
- Persisted on disk and reused forever (sticker images are immutable per id). A cached file that
  fails to decode was deleted so the next request refetches. [I]
- Catalog (`store_packs`) was cached and reused when it held ≥ 20 packs; pack detail cached per
  `cateId` when non-empty; suggest keywords cached when ≥ 100 entries; keyword search results
  cached per `md5(normalize(keyword))`. None of these caches had an expiry — a rewrite should add
  one (catalog and keywords change over time). [I]
- Cache is not secret but was shared across accounts; that is harmless for stickers. [I]

## Open questions

- Meaning of sticker `type` values (7 default, 1 seen in a fixture); does sending the wrong
  `type` fail? [U]
- How animated stickers are delivered (sprite sheet, animated WebP, Lottie, …) and from which
  URL. The previous client only ever rendered a static frame. [U]
- Other valid `size` values for `webpc`, and whether that host needs cookies/Referer. [U]
- Meaning of `gif`, `guggy` (suggest) and `srcType` (search). [U]
- Path conflict: implemented `…/sticker/category/sticker_detail` vs the endpoint table in
  an earlier host table listing `…/sticker/sticker_detail`. Neither has a
  recorded capture. [U]
- Why the client avoided `tt-chat2-wpa` hosts for 1-1 sticker sends. [U]
- Error codes specific to sticker endpoints (invalid sticker, pack not owned, etc.). [U]
- Exact nesting of decrypted sticker responses (`data.keywords` vs `keywords`, `data` array vs
  bare array). [U]
- Numeric `msgType` for stickers: one mapping says `5`, the archive mapping says `10`; see
  [../client-rules/message-model.md](../client-rules/message-model.md). [U]
