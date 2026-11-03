# Password login

Logs in with phone number and password through `loginByPasswv2`. The password is encrypted with `PasswordCipher` and the whole payload is wrapped in ParamCipher v2. This doc also lists the other login-group endpoints seen in the bundle.

Evidence tags: `[O]` observed/live-verified, `[I]` implemented in the previous client without recorded live evidence, `[U]` unknown.

**Status:** the crypto layer is byte-exact (a live response decrypted correctly), and the signed field set matches a captured working request. No end-to-end successful password login was recorded. The last recorded live reply was `18022`, and the field-set fix was applied after it. [O] for the crypto, [U] for the full success path. QR login ([qr-login.md](qr-login.md)) is the proven path.

## Flow

```
getServerInfo                      (service-discovery.md)
  → [getCaptcha if required]       (shape unknown)
  → loginByPasswv2                 → session cookies (zpw_sek) + login data
  → getLoginInfo                   → zpw_enk, uid, endpoints  (session.md)
  → persist session
```

## PasswordCipher

Constant shipped in the bundle: [O]

```
PASSWORD_CIPHER_KEY = "9fc5vEW3"     # input string for key derivation, NOT a raw AES key
```

Algorithm (`class N` in the bundle): [O]

```
salt     = random string, 8 chars           # previous client: [a-z0-9]{8}  [I]
src      = UPPER_HEX( sha256( utf8("{PASSWORD_CIPHER_KEY}-{phone}-{imei}-{salt}") ) )   # 64 chars
enk      = even(src)[0:16] + reverse(odd(src))[0:16]       # 32 ASCII chars → AES-256 key
password_enc = BASE64( AES_CBC(utf8(enk), iv=0^16, PKCS7, utf8(salt + reverse(password))) )
```

- `even` / `odd` are the characters at even and odd indices. `reverse(password)` reverses the characters, not the bytes. [O]
- `phone` is the same string sent as `phone_num`. [I] The normalisation (leading `0` vs country prefix) has not been checked.
- There is no RSA on the password path. [O]

### loginByPasswv2
- **Request:** `POST {auth}/api/login/loginByPasswv2` (default host: `https://wpa.zaloapp.com`). cmd `11176`. [O]
- **Encoding:** ParamCipher v2 ([../security/param-cipher.md](../security/param-cipher.md)), signkey tag `loginbypasswv2`. **All fields go in the URL query** (`zcid`, `zcid_ext`, `enc_ver`, `params`, `type`, `client_version`, `signkey`). The POST body is empty. `ref=https://chat.zalo.me` is appended **after** signing and is not in the hash. [O]
- **Params (plaintext, before encryption, i.e. the object inside `params`):**

  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `phone_num` | string | yes | phone number |
  | `iso_country_code` | string | yes | e.g. `VN` |
  | `password` | string | yes | `password_enc` from PasswordCipher |
  | `salt` | string | yes | the salt used by PasswordCipher. Must be set **before** encryption. [O] |
  | `cptch_token` | string | yes | captcha token, `""` when no captcha |
  | `cptch_answer` | string | yes | captcha answer, `""` when no captcha |
  | `make_answer` | int | yes | `0` [I] meaning unknown |
  | `language` | string | yes | previous client sent `en` [I] |
  | `logged_uids` | array \| null | yes | stored uid list, or `null` when none. Sending the string `"[]"` changes the payload. [I] |
  | `computer_name` | string | yes | device name |
  | `imei` | string | yes | device id |
  | `quest_cert` | string | no | only when present (security-question certificate) [O] |
  | `cptch_cert` | string | no | only when present (captcha certificate) [O] |
  | `last_uid` | string | no | only when present [O] |
  | `viewerkey` | string | no | only when present [O] |

  Do **not** add fields the bundle does not send. Adding `client_time` to this payload was one cause of `18022`. [O]

- **Response:** outer `{error_code: 0, data: "<base64>"}`. Decrypt `data` with the ParamCipher request key to get the real envelope `{error_code, error_message, data}`. [O] On success, inner `data` is expected to carry login config (`zpw_enk`, uid, endpoint keys), handled like `getLoginInfo`, and the response sets the `zpw_sek` cookie. [I] Then call `getLoginInfo`. [I]
- **Errors:**

  | Code | Where | Meaning |
  |---|---|---|
  | `18060` | outer | `Invalid encryption protocol`: sent with plain `params=`. A **protocol** rejection: the password was never checked, so do not report a wrong password. [O] |
  | `18022` | inner | `Invalid Signature`: wrong signed field set, e.g. `ref` hashed or extra payload fields. [O] |
  | others | inner | wrong credentials, captcha required, etc. Codes not recorded. [U] |

- **Bundle source (verbatim order):** [O]

  ```
  g = {phone_num, iso_country_code, password, cptch_token, cptch_answer,
       make_answer, language, logged_uids, computer_name, imei}
  if enable_encrypt_param: g.salt = salt; g.password = encryptPassword(password)
  if quest_cert: g.quest_cert;  if cptch_cert: g.cptch_cert
  if last_uid:  g.last_uid;     if viewerkey:  g.viewerkey
  {params f, key} = encryptParam(g, "loginbypasswv2")
  f.ref = origin                      # after signing
  POST authDomain + "/api/login/loginByPasswv2?" + urlParams(f)
  decryptResp(key, response)
  ```

### loginByPassw (v1, legacy)
- **Request:** `POST {auth}/api/login/loginByPassw` [O] (it exists in the bundle)
- **Encoding:** plain. Form body `params=<urlencoded JSON>`, where the JSON object includes its own `signkey` (tag `loginbypassw`). `ref` is set **before** signing and is hashed. [I]
- **Params:** `{phone_num, password, cptch_token, cptch_answer, type: 30, client_version: 694, computer_name, imei, ref: "https://chat.zalo.me", signkey}`. The password is sent as given. [I]
- **Errors / Notes:** it is not known whether the server still accepts v1 for any account. Since plain params to v2 return `18060`, it probably does not. Keep v1 only as a diagnostic. [U]

### getCaptcha
- **Request:** `POST {auth}/api/login/getCaptcha`, cmd `11325`. [O] (it exists)
- **Params / response:** unknown. Needed only after failed attempts. The result feeds `cptch_token` / `cptch_answer` / `cptch_cert`. [U]

## Other login-group endpoints (bundle inventory)

All live on the auth host under `/api/login/` unless a full path is shown. Only the ones marked as used were exercised. [O] (they exist in the bundle)

| Path | cmd | Notes |
|---|---|---|
| `reqqr` | 11133 | QR login, see [qr-login.md](qr-login.md) (used) |
| `getLoginInfo` | 11134 | session setup, see [session.md](session.md) (used) |
| `logOut`, `/api/v2/login/logOut` | 11135 | logout |
| `loginByPasswv2` | 11176 | this doc (used) |
| `getServerInfo` | 11323 | see [service-discovery.md](service-discovery.md) (used) |
| `getCaptcha` | 11325 | captcha |
| `setting` | 12113 | user settings |
| `loginByResetPwd`, `actlistv2`, `chkUpdate`, `popup`, `getSupportCountry`, `hasNewMsg`, `getActiveCodeByZalo`, `getFbAuthInfo`, `fbAuth`, `linkFacebook`, `loginByFacebook`, `loginByFacebookWithSecure`, `fblogout`, `migrateSS`, `ssoSyncMp3`, `completeRemoveData` | — | not used |
| `/api/enterprise/login/emailPassword` | — | enterprise login, not used |

## Open questions

- [U] An end-to-end successful `loginByPasswv2` response: inner `data` shape and which cookies it sets.
- [U] Error codes for wrong password, locked account and captcha-required.
- [U] The `getCaptcha` request and response, and how `cptch_token`, `cptch_answer` and `cptch_cert` relate.
- [U] The meaning of `make_answer`, `quest_cert`, `viewerkey` and `last_uid`.
- [U] Phone-number normalisation for `phone_num` and for the PasswordCipher input.
- [U] Whether `language` should be `vi` (the previous client sent `en` here and `vi` elsewhere).
- [U] An early checklist mentioned "handle `19000` → retry plain". Nothing records what `19000` means.
