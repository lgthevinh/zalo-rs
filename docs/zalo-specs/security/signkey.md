# signkey — request signature

`signkey` is an MD5 signature over a request's parameter object. The pre-auth and login endpoints reject unsigned or wrongly signed requests; this doc defines the algorithm, how tags are named, and which fields fall inside the hash.

Evidence tags: `[O]` observed/live-verified, `[I]` implemented in the previous client without recorded live evidence, `[U]` unknown.

## Algorithm

```
signkey(tag, params):
    s = "zsecure" + tag
    for key in sort(keys(params)):          # plain byte-wise ascending sort of key names
        s += stringify(params[key])
    return lowercase_hex(md5(utf8(s)))      # 32 lowercase hex chars
```

- Only the **values** are concatenated, with no separators and no key names. [O]
- Keys are sorted by name. Insertion order does not matter. [O]
- `stringify`: strings are used verbatim, before any URL encoding. Integers are written as decimal (`-1` → `"-1"`). [O] The previous client wrote `null` as `"null"` and booleans as `"true"`/`"false"` (the same as JavaScript `String(v)`). No signed request is known to contain a `null`, a boolean, an array or an object, so these cases have not been checked against the server. [I]
- The signature is added to the request as the `signkey` field **after** it is computed. It is never part of its own input. [O]

The signature is computed over the decoded values, but the server checks it against the values it decodes from the query or form. Values must therefore round-trip exactly through URL encoding (see [Open questions](#open-questions)).

## Tag naming

The tag is the last path segment of the endpoint, lowercased, with dashes removed: [O]

| Endpoint | Tag | Evidence |
|---|---|---|
| `/api/login/getServerInfo` | `getserverinfo` | [O] live: unsigned → `18022`, signed → `0` |
| `/api/login/reqqr` | `reqqr` | [O] live, plain query accepted |
| `/api/login/loginByPasswv2` | `loginbypasswv2` | [O] signed field set confirmed against a captured working request |
| `/api/login/getLoginInfo` | `getlogininfo` | [O] works on every QR login |
| `/api/login/loginByPassw` (v1) | `loginbypassw` | [I] |

An earlier bundle note said the tag in the v2 path is the numeric command id (e.g. `"11176"`). Every live-verified call above uses the lowercased endpoint name instead. Use the name. [O]

## What is signed: the "added after signing" rule

**Any field added to the request after `signkey` is computed is not covered by the signature.** The server recomputes the signature over a fixed field set, so adding an extra field to the hash, or leaving out a required one, returns `18022 Invalid Signature`. [O]

Known post-signature fields, which must **not** go into the hash: [O]

| Field | Appears on | Value |
|---|---|---|
| `ref` | `loginByPasswv2` | page origin, `https://chat.zalo.me` |
| `nretry` | `getLoginInfo` | retry counter, `0` on the first try |

### Signed field sets per request shape

1. **Plain signed request** (`getServerInfo`, `reqqr`). Every parameter goes into the hash, including `type` and `client_version`. [O]
   - `getServerInfo`: `{imei, type, client_version, computer_name, sp_mtn, bkt}`.
   - `reqqr`: `{language, client_time, imei, computer_name, logged_uids, self_g, qr_form, type, client_version}`.
2. **ParamCipher v2 request** (`loginByPasswv2`, `getLoginInfo`). The hash covers the **outer**, post-encryption object only: [O]

   ```
   {client_version, enc_ver, params, type, zcid, zcid_ext}
   ```

   Sorted, this concatenates `client_version + enc_ver + params + type + zcid + zcid_ext`. The fields inside the encrypted `params` are not signed one by one. They are covered only through the ciphertext. See [param-cipher.md](param-cipher.md).
3. **v1 password login** (`loginByPassw`, legacy). The plain parameter object is signed, and `ref` is set **before** signing, so it is inside the hash. [I] This is the opposite of the v2 rule.
4. **Authenticated session-AES calls** (everything after login, see [payload-encryption.md](payload-encryption.md)) carry **no** `signkey`. They are authenticated by the `zpw_sek` cookie. [O] These calls work live without a signature.

A field that the encrypted v2 payload had to contain (`salt`) belongs inside `params`. A field that the bundle never sent (`client_time` in the password payload) caused `18022` when it was added. Field-set drift, not the hash algorithm, was the cause of every signature failure seen so far. [O]

## Test vectors (synthetic inputs)

| tag | params | string hashed | signkey |
|---|---|---|---|
| `reqqr` | `{language:"vi", type:"30", client_version:"693"}` | `zsecurereqqr693vi30` | `e744681ce317d46eb39f5622076db478` |
| `10053` | `{b:"2", a:"1"}` | `zsecure1005312` | `815e0c6031b6e8c37d0c4babb2b02337` |
| `TAG` | `{zcid:"A", enc_ver:"v2", type:"30", params:"P", client_version:"693", zcid_ext:"B"}` | `zsecureTAG693v2P30AB` | `c1c5979a8fb4574016d39ac7e59b7b26` |

## Errors

| `error_code` | Meaning |
|---|---|
| `18022` | `Invalid Signature`: wrong tag, wrong field set, or a post-signature field was hashed. [O] On ParamCipher requests this code arrives **inside** the encrypted `data` envelope while the outer `error_code` is `0`. |

See [../client-rules/errors.md](../client-rules/errors.md).

## Open questions

- [U] How the server stringifies arrays and objects for the hash (JSON vs JavaScript `String()`, which gives `"a,b"` and `"[object Object]"`). This does not matter as long as signed fields stay scalar.
- [U] URL encoding: the previous client percent-encodes everything except RFC 3986 unreserved characters. The bundle uses JavaScript `encodeURIComponent`, which also leaves `!'()*` unescaped. No mismatch has been seen, but a value containing those characters has not been tested.
- [U] Whether any authenticated endpoint other than the login group requires `signkey`.
