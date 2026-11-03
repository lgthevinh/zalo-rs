# Capturing and verifying official client traffic

How to capture a request from the official Zalo PC/Web client, and how to check it against these specs by hand. Use this whenever a request is rejected (`18022`, `18060`, `600`) or before changing any params, signing or cipher behaviour.

Evidence tags: `[O]` observed/live-verified, `[I]` implemented in the previous client without recorded live evidence, `[U]` unknown.

> Use a throwaway account. Captures contain session material.

## 1. Capture one request

You usually need **one** request, e.g. the `loginByPasswv2` sent when you press *Log in*, or one failing data call. Copy its **full URL** (and the form body for POSTs).

### Option A: mitmproxy

```
mitmdump -w zalo.flow
```

1. Set the system proxy to `127.0.0.1:8080`, then install the mitmproxy CA from `http://mitm.it`.
2. Do the action in Zalo PC, then stop mitmdump.
3. Filter: `mitmdump -nr zalo.flow "~u loginByPasswv2" -w only.flow`, then copy the URL (and body).

### Option B: Fiddler Classic

Enable HTTPS decryption (Tools → Options → HTTPS → Decrypt). Do the action, filter on the endpoint name, then **Copy → Just Url**. Fiddler installs its own certificate, so Zalo PC usually accepts it.

### Option C: in-app fetch hook (no proxy)

This works only if DevTools opens (`Ctrl+Shift+I`). Release builds usually block it.

```js
const _fetch = window.fetch;
window.fetch = function (...a) {
  const u = typeof a[0] === 'string' ? a[0] : a[0].url;
  if (u && u.includes('loginByPasswv2')) { console.log(u); copy(u); }
  return _fetch.apply(this, a);
};
```

## 2. Extract the fields

Split the query on `&`, then URL-decode each value. Decode `%XX`; treat `+` as a space only in form bodies. Write down:

| Field | Present on | Use |
|---|---|---|
| `zpw_ver`, `zpw_type` | data calls, WebSocket | compare with [../auth/session.md](../auth/session.md#common-params) |
| `type`, `client_version` | login-group calls | same values as above |
| `zcid`, `zcid_ext`, `enc_ver` | ParamCipher v2 calls | step 3 |
| `params` | ParamCipher and session-AES calls | step 3 / 5 |
| `signkey` | login-group calls | step 4 |
| `ref`, `nretry` | some login calls | must **not** be signed |
| `enk` | expected never (see [../security/param-cipher.md](../security/param-cipher.md#enk-query-parameter)) | record it if present |
| headers `Cookie`, `Origin`, `Referer`, `User-Agent` | all | compare with session.md |

## 3. Decrypt a ParamCipher v2 request

Follow [../security/param-cipher.md](../security/param-cipher.md):

1. **`zcid` → plaintext.** Hex-decode `zcid`, then AES-256-CBC decrypt it (zero IV, PKCS#7) with the ASCII key `3FC4F0D2AB50057BCE0D90D9187A22B1`. You should get `"{type},{imei},{firstLaunchTime}"`. This shows the official client's `imei` shape and `firstLaunchTime` unit.
2. **Re-derive the request key** from `zcid` and `zcid_ext`: `even(UPPER(md5_hex(zcid_ext)))[0:8] + even(zcid)[0:12] + reverse(odd(zcid))[0:12]`.
3. **Decrypt `params`** (base64 → AES-256-CBC, zero IV, PKCS#7) with that key. The result is the exact plaintext field list the server receives. Compare it field by field with the endpoint doc, including which optional fields are present and the value types (`null` vs `"[]"`, int vs string).
4. The same key decrypts the response `data`. Read the **inner** `error_code`.

If step 3 fails with a padding error, the key derivation or the input values are wrong.

## 4. Recompute `signkey`

Follow [../security/signkey.md](../security/signkey.md): `md5("zsecure" + tag + values sorted by key)`, using the **decoded** values. Try these field sets until one matches the captured `signkey`:

| Variant | Fields (sorted) |
|---|---|
| ParamCipher standard | `client_version, enc_ver, params, type, zcid, zcid_ext` |
| with `enk` | the above + `enk` |
| without `params` | `client_version, enc_ver, type, zcid, zcid_ext` |
| with `ref` | the standard set + `ref` |
| plain request | every query field except `signkey` (and except `ref`/`nretry` on v2) |

The matching variant tells you the signed set. For `loginByPasswv2` the standard set matched a working request. [O] If nothing matches, try other tags: the lowercased endpoint name, or the numeric cmd id.

## 5. Decrypt a session-AES (post-login) request

You need the session's `zpw_enk` from the captured `getLoginInfo` response (decrypt it first, as in step 3). Then base64-decode and AES-CBC decrypt `params` and the response `data` with it. See [../security/payload-encryption.md](../security/payload-encryption.md).

## 6. Record the evidence

- Put a sanitised summary into the relevant spec doc, tagged `[O]` with the date and Zalo version.
- Before committing anything, replace real tokens, cookies, `zpw_sek`, `zpw_enk`, `imei`, phone numbers and user/group ids with placeholders (`<imei>`, `<uid>`, …).
- Never commit raw capture files or dumps. Change the throwaway account's password afterwards.

## Open questions

None known.
