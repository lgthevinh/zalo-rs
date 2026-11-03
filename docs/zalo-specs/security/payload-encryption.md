# Session payload encryption (`zpw_enk`)

After login, data endpoints encrypt their `params` with a per-session AES key, `zpw_enk`, and the server encrypts the response `data` with the same key. This doc covers parsing that key, encrypting requests, decrypting responses, and when each applies.

Evidence tags: `[O]` observed/live-verified, `[I]` implemented in the previous client without recorded live evidence, `[U]` unknown.

## Key source

- `zpw_enk` is a field of the `getLoginInfo` response `data`. See [../auth/session.md](../auth/session.md). [O]
- It is per session. Store it with the session. No hardcoded key exists for this layer. [O]
- In the bundle it is the "secret key" used by `encodeAES`/`decodeAES` (constant `secretKey = "zpw_enk"`, which the bundle also calls a "legacy md5 sign key"). [O] The bundle reads it with `Base64.parse`, which means it is base64 on the wire. [O]

### Key parsing

```
parse_secret_key(zpw_enk):
    t = trim(zpw_enk)
    if len(t) in (16, 32):            # already raw ASCII key material
        return ascii_bytes(t)
    raw = BASE64_DECODE(t)            # standard alphabet, with padding
    if decode ok and len(raw) in (16, 32):
        return raw
    return ascii_bytes(t)             # will fail at AES time; treat as a bad session
```

- 16-byte key → **AES-128-CBC**. 32-byte key → **AES-256-CBC**. Any other length is an error. [I]
- Observed sessions: the value arrived base64 and decoded to 32 bytes, so AES-256. [O] Another project doc described `zpw_enk` as an AES-128 key, and the client accepts both. Which lengths the server actually issues is still open. [U]
- Raw and base64 forms cannot be confused: base64 of 16 bytes is 24 chars and base64 of 32 bytes is 44 chars, never 16 or 32.

## Cipher

`AES-CBC`, **16-byte all-zero IV**, **PKCS#7** padding, ciphertext encoded as **standard base64**. The output is deterministic: the same plaintext always gives the same ciphertext. [O]

```
encodeAES(key, text) = BASE64( AES_CBC_ENCRYPT(key, iv=0^16, PKCS7(utf8(text))) )
decodeAES(key, b64)  = utf8( PKCS7_UNPAD( AES_CBC_DECRYPT(key, iv=0^16, BASE64_DECODE(trim(b64))) ) )
```

## Request encryption

```
params = urlencode( encodeAES(zpw_enk_key, JSON(payload)) )
GET  {service}/api/<path>?zpw_ver=694&zpw_type=30&params=<params>[&extra unencrypted query fields]
POST {service}/api/<path>?zpw_ver=694&zpw_type=30      body: params=<params>
     Content-Type: application/x-www-form-urlencoded
```

- The common params `zpw_ver` / `zpw_type` always go in the query, unencrypted. [O]
- Some endpoints take extra plain query fields beside `params`. For example, the profile `me-v2` call adds `os=8`. Per-endpoint docs under `api/` list these. [I]
- No `signkey` on these calls. They are authenticated by the `zpw_sek` cookie. See [signkey.md](signkey.md) and [../auth/session.md](../auth/session.md). [O]
- The previous client fell back to unencrypted `params=<urlencoded JSON>` on a few read endpoints when no session key was loaded. Whether the server accepts that for an authenticated session has not been checked. [U]

## Response decryption

```
resp = JSON(body)
d = resp.data
if d is a string and d != "" and not d.startswith("{"):
    plaintext = decodeAES(zpw_enk_key, d)
    inner = JSON(plaintext)
    # treat `inner` as the result; it may itself be an envelope {error_code, error_message, data}
```

- Encrypted responses arrive as `{"error_code":0, "data":"<base64>"}`. Decrypt `data` before reading anything else. [O]
- Readers should accept both shapes after decryption: a full envelope (`inner.error_code`, `inner.data.…`) and a bare payload (`inner.…`). The previous client checked both, for example `inner.data.profile` with a fallback to `inner.profile`. [I] For ParamCipher responses the inner value is always an envelope. [O] See [param-cipher.md](param-cipher.md).
- If decryption or JSON parsing fails, keep the outer response as-is. Do not report success. [I]
- Bundle rule: responses are encrypted only when the request was encrypted. Plain requests get plain JSON back. [O]
- An empty HTTP 200 body (long-poll keepalive) is not an encrypted payload. Treat it as `{error_code:0, data:null}`. [O] See `realtime/` docs.

## Compression

The HTTP session-AES path has **no compression layer**: decrypted plaintext is UTF-8 JSON. [O] Compression (gzip/zlib/deflate) appears only on the WebSocket transport (`encrypt` modes 1–3) and on some CDN downloads. Those are covered in `realtime/` and `media/` docs.

## Errors

| `error_code` | Meaning |
|---|---|
| `600` | `zpw_sek not found or invalid`: missing or expired session cookie. [O] |
| `-102` | Treated as session expired by the previous client. [I] |

The full table is in [../client-rules/errors.md](../client-rules/errors.md).

## Open questions

- [U] Whether the server ever issues a 16-byte (AES-128) `zpw_enk`, or always 32 bytes.
- [U] Some responses may arrive as a bare base64 body rather than a JSON envelope. The previous client noted this case for `me-v2` but only decrypted the `data` field, so the bare case is unconfirmed.
- [U] Whether unencrypted `params=` is still accepted on any authenticated endpoint.
