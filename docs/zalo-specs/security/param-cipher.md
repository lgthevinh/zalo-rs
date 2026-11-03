# ParamCipher v2 — per-request parameter encryption

ParamCipher v2 (`enc_ver=v2`) encrypts the parameters of the login-group endpoints with a key derived fresh for each request, from a static key shipped in Zalo's bundle plus random material. The same derived key decrypts the response `data`.

Evidence tags: `[O]` observed/live-verified, `[I]` implemented in the previous client without recorded live evidence, `[U]` unknown.

## When it applies

| Endpoint | Uses ParamCipher v2 |
|---|---|
| `POST /api/login/loginByPasswv2` | yes. **Mandatory**: sending plain `params=` returns `18060 Invalid encryption protocol`. [O] |
| `GET /api/login/getLoginInfo` | yes [O] (works on every QR login) |
| `GET /api/login/getServerInfo` | no: plain signed query [O] |
| `GET /api/login/reqqr` | no: plain signed query accepted [O] |
| Post-login data endpoints | no: these use session AES with `zpw_enk`, see [payload-encryption.md](payload-encryption.md) |

In the bundle the gate is the server-config flag `enable_encrypt_param` (from `getServerInfo`). In practice it is always on: plain requests to `loginByPasswv2` get `18060`. Treat ParamCipher as mandatory for the endpoints above. [O]

## Constants

```
PARAM_CIPHER_KEY = "3FC4F0D2AB50057BCE0D90D9187A22B1"   # 32 ASCII chars, used as raw AES-256 key bytes [O]
ENC_VER          = "v2"
type             = 30        # same value as zpw_type
client_version   = 694       # same value as zpw_ver; see auth/session.md
```

## Primitives

- `AES_CBC(key, plaintext)`: AES-CBC with a 16-byte **all-zero IV** and PKCS#7 padding. Key length picks AES-256 (32 bytes) here. Keys are the **ASCII bytes** of the 32-char strings, not hex-decoded. [O]
- `md5_hex`: lowercase hex MD5.
- `even(s)` / `odd(s)`: the characters of `s` at indices 0,2,4,… and 1,3,5,… (the bundle calls this `processStr`). [O]

## Algorithm

```
# 1. Client id (deterministic per device)
zcid = UPPER_HEX( AES_CBC(PARAM_CIPHER_KEY, utf8("{type},{imei},{firstLaunchTime}")) )

# 2. Per-request randomness
zcid_ext = random lowercase hex string, 6..12 chars       # bundle: Math.random().toString(16) slice

# 3. Derive the request key (32 ASCII chars)
n   = UPPER( md5_hex(zcid_ext) )
key = even(n)[0:8]  +  even(zcid)[0:12]  +  reverse(odd(zcid))[0:12]

# 4. Encrypt the payload
params = BASE64( AES_CBC(utf8(key), utf8(JSON(payload))) )

# 5. Assemble and sign
m = { zcid, zcid_ext, enc_ver: "v2", params, type: "30", client_version: "694" }
m.signkey = signkey(tag, m)               # see signkey.md: signs exactly these six fields

# 6. Append unsigned extras (e.g. ref, nretry) AFTER signing
# 7. Send: every field of m as a URL query parameter (flattened), not wrapped in `params=`
```

Notes:

- `zcid` is upper-case hex. For an `imei` of the usual 69-char shape the plaintext is about 80 bytes, which gives 96 bytes of ciphertext and a 192-char `zcid`. [O] (format) / [I] (length arithmetic)
- `firstLaunchTime`: in the bundle this is the app's first-launch timestamp. The previous client passed the current Unix time in **seconds** for every request, and the server accepted it: the response decrypted correctly and the request reached signature and business validation. [O] Whether the server checks the value or its unit is unknown. [U]
- `zcid_ext` length: the bundle produces 6 to 12 hex chars. The previous client always used 10. [I]
- The key mixes 8 chars from the MD5 of `zcid_ext`, 12 even-index chars of `zcid`, and 12 chars of the reversed odd-index chars of `zcid`. This derivation is byte-exact: a real `loginByPasswv2` response decrypted cleanly with it. [O]
- `payload` is the endpoint's plaintext parameter object (see [../auth/password-login.md](../auth/password-login.md) and [../auth/session.md](../auth/session.md)). Serialise it as compact JSON.

## Response decryption

```
resp = HTTP response JSON                     # outer envelope, usually error_code 0
if resp.data is a non-empty string not starting with "{":
    inner = JSON( AES_CBC_DECRYPT(utf8(key), BASE64_DECODE(resp.data)) )
    # inner is itself a full envelope: {error_code, error_message, data}
    use inner as the real result
```

- **The derived `key` is the only key.** It encrypts `params` and also decrypts the response. [O] (verified on a live Web session and a captured login response)
- **The outer `error_code` can be `0` while the inner one is not.** A real `loginByPasswv2` reply had outer `error_code 0` and an inner `{"error_code":18022,"error_message":"Invalid Signature","data":null}`. Always decrypt first, then read the inner `error_code`. [O]

## `enk` query parameter

Early bundle notes say the derived key travels in the query as `enk` "so the server can encrypt the response". Later live checks against Zalo Web found **no `enk` parameter** in real requests. The server decrypts the response key from `zcid` / `zcid_ext` on its own. The previous client never sent `enk`, and its requests work. [O] Do not send `enk`. Never put it in the signature.

## Errors

| `error_code` | Where | Meaning |
|---|---|---|
| `18060` | outer | `Invalid encryption protocol`: the endpoint requires ParamCipher and got plain params. Says nothing about credentials. [O] |
| `18022` | inner (inside encrypted `data`) | `Invalid Signature`: see [signkey.md](signkey.md). [O] |

## Appendix: static constants in the Zalo PC bundle

These come from the bundle's secrets module (`ZPC_SECRET_VALUES`, Zalo PC 26.10.11). They are protocol constants shipped to every client, not per-user secrets. [O] Only the first two are used on the login and session paths. The others are recorded for future work (sync-v2, calls, local storage).

| Name | Value | Use |
|---|---|---|
| `paramCipherKey` | `3FC4F0D2AB50057BCE0D90D9187A22B1` | ParamCipher v2 (this doc), raw 32-byte AES-256 key |
| `passwordCipherKey` | `9fc5vEW3` | PasswordCipher key-derivation input, see [../auth/password-login.md](../auth/password-login.md) |
| `timeOnAppXorKey` | `IUU3RM3GABH3NS0ABD3TT04N9ABCAA1U` | unknown [U] |
| `zsafeStorageKey` | `afe75eea0d3c4bd9a65439a54eabc988` | local safe storage [I] |
| `syncV2TestPrivateKeys.pcik` | `8CogpzApFRI1AeeaGqS/j3lFtKzLZcUjQxlH843yE30=` | base64 32 B, possibly Ed25519 [I] |
| `syncV2TestPrivateKeys.pcek` | `IEQpDf07mUSdOtfj2+fsme5jkcxIj5oc2DqMDI9q+Wk=` | base64 32 B [I] |
| `legacyDataUtilsAesKey` | `eEH6Qo6+URg6QCZoYnQ+pQ==` | unknown [U] |
| `genServicesDecodeAesKey` | `LCEW+XgyWHheUbHs4bSm4Q==` | unknown [U] |
| `genServicesDecryptAesKey` | `b419830e37258e92d57adcfb0c65920d` | unknown [U] |
| `callV2TransportAesKey` | `yjAF9oqMWl6XfXYJn9mA7w==` | call transport [I], see `call/` docs |
| `legacyNativeMigrationAesKey` | `S2h1ZUJlb0RvbTE5MDg4NQ==` | migration [I] |
| `ZLOG_MODULE_KEY` | key `vOTH6sd9pNWjRriqCc7rDxs01lwHzfr3`, iv `7eb5dbe084b7eedeef04e2622d46ba00` | log encryption [I] |

The bundle also ships a third-party maps API key. It is irrelevant to the protocol and is left out here.

Bundle crypto bridge defaults: `encrypt(text, algo, key, iv = 16 zero bytes)`, PKCS#7, string keys taken as UTF-8 bytes, output hex or base64. `createHash("sha256")` is available. RSA `publicEncrypt`/`privateDecrypt` are exposed for E2EE paths and are not used in login. [O]

## Open questions

- [U] Whether `firstLaunchTime` must stay the same for a device across requests and sessions (the previous client sent the current time on each request) and whether it is in seconds or milliseconds.
- [U] Whether `zcid_ext` lengths outside 6..12 are rejected.
- [U] The "`enk` travels in the query" claim from early notes conflicts with later live observation. The bundle code path that produced it has not been re-checked.
