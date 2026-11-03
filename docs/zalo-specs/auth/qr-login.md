# QR login

QR login asks the auth server for a one-time QR token, shows it to the user, long-polls until the phone scans the code, then long-polls until the user confirms on the phone. The confirmation yields the `zpw_sek` session cookie. This is the login path the previous client used day to day, and it works end to end live.

Evidence tags: `[O]` observed/live-verified (live run 2026-10-20 unless noted), `[I]` implemented in the previous client without recorded live evidence, `[U]` unknown.

## Flow

```
getServerInfo (optional, see service-discovery.md)
  → reqqr                    → token + polling URLs
  → render QR (qr_cnt or base64_qr)
  → long-poll chk_wait_scan  → scanned (preview of the scanning account)
  → long-poll chk_wait_cfirm → confirmed; Set-Cookie zpw_sek
  → getLoginInfo             → zpw_enk, uid, endpoints   (see session.md)
  → persist session
```

Bundle path: `getQRCode` → `genFirstQR` → `verifyDataWaitingScan` → `genUrlWaitingConfirm`. [O]

## Endpoints

### Request QR (reqqr)
- **Request:** `GET {auth}/api/login/reqqr` (default host: `https://wpa.zaloapp.com`). cmd `11133`. [O]
- **Encoding:** plain query, signkey tag `reqqr`. The signature covers every field below except `signkey`, including `type` and `client_version`. The plain form is accepted. [O]
- **Params:**

  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `language` | string | yes | `vi` |
  | `client_time` | int | yes | Unix time, seconds [I] (the unit was not checked against the bundle) |
  | `imei` | string | yes | device id, see [session.md](session.md#device-identity-imei) |
  | `computer_name` | string | yes | device name |
  | `logged_uids` | string | yes | previously logged-in uids. The previous client sent the string `[]`, which was accepted. [O] |
  | `self_g` | int | yes | `1` → server returns the QR **payload** (`qr_cnt`). `0` → server returns a rendered PNG (`base64_qr`). [O] |
  | `qr_form` | int | yes | `0` |
  | `type` | int | yes | `30` |
  | `client_version` | int | yes | `694` (live run used `693`) |
  | `signkey` | string | yes | `md5("zsecure" + "reqqr" + values sorted by key)` |

- **Response `data`:** plain JSON (snake_case): [O]

  | Field | Example (sanitised) | Meaning |
  |---|---|---|
  | `token_id` | `zaloqr:v2_<token>` | QR token |
  | `qr_cnt` | `http://zaloapp.com/qr/l?tk=zaloqr:v2_<token>` | Text to encode in the QR (`self_g=1`). It contains `token_id`. |
  | `base64_qr` | `<PNG base64, ~22 KB>` | Server-rendered QR (`self_g=0`). It encodes the same URL. |
  | `qr_timeout` | `180` | Seconds the code stays valid |
  | `qr_retry` | `3` | Allowed re-polls on error |
  | `chk_wait_scan` | `https://qr-id.zaloapp.com/polling/qr/waiting?src=pc&code=<code>` | Scan long-poll URL |
  | `chk_wait_cfirm` | `https://qr-id.zaloapp.com/polling/qr/pc-status?src=pc&code=<code>` | Confirm long-poll URL |

- **Errors:** a non-zero `error_code` is fatal for this attempt. A response with no `chk_wait_scan`, or with neither `qr_cnt` nor `base64_qr`, is invalid. [I]
- **Notes:** the server accepted `self_g=1` even though its own config says `self_g: 0`. [O] The polling host `qr-id.zaloapp.com` is handed out by the server. Use the returned URLs as-is and never build them. [O] Encrypting `reqqr` with ParamCipher was tried with several tags, but the result was never recorded. Use the plain form. [U]

### Wait for scan
- **Request:** `GET <chk_wait_scan>&lang=vi` [O]
- **Encoding:** none. The URL comes from the server.
- **Response:**
  - **HTTP 408**: nobody has scanned yet, and the long poll timed out. **This is "keep waiting", not an error.** Poll again. [O]
  - **HTTP 200, `error_code 0`, non-null `data`**: scanned. `data` carries a preview of the scanning account (`display_name`, avatar, `link_data`). [I]
  - **HTTP 200, non-zero `error_code`**: count it as a retry. Give up after `qr_retry` attempts. [I]
  - Any other HTTP status: fail. [I]
- **Notes:** loop until `qr_timeout` seconds pass, then ask for a new QR. The previous client waited about 0.8 s between polls and used `max(qr_timeout, 30)` as its deadline. [I]

### Wait for confirm
- **Request:** `GET <chk_wait_cfirm>` [O]
- **Encoding:** none.
- **Response:**
  - **HTTP 408**: keep polling. [I]
  - **HTTP 200, `error_code 18012` (`Invalid status`)**: not confirmed yet. Keep polling. [O] (seen before a scan) / [I] (after a scan, waiting for the user to tap confirm)
  - **HTTP 200, `error_code 0`**: confirmed. Take `zpw_sek` from the response's `Set-Cookie: zpw_sek=<value>` (ignore the value `EXPIRED`). [I] The previous client also checked the body (`zpw_sek`, `data.zpw_sek`, `data.account.zpw_sek`, and the names `zpw_kt`, `zpw_wpsk`, `token`, `session_key`) as fallbacks. Which of these the server really uses is unknown. [U]
  - Any other `error_code` or HTTP status: fail.
- **Notes:** deadline `max(qr_timeout, 60)` s, polling every ~0.8 s. [I] After success, attach `zpw_sek` to every service host (see [session.md](session.md#cookies)), then call `getLoginInfo` to get `zpw_enk`, uid and endpoints, and persist the session. This sequence works live. [O]

## Trust device

`link_data = {enc_link_data, enc_link_hmac}` in the scan preview feeds the bundle's `createCompanionLinkingProof` → `encLinkingMetaData` / `encAccountSign` / `encDeviceSign` / `encPK`. Those values are appended to the confirm URL as a device-linking proof. [I] They are only produced when the server config `setttings.authen.trusted_device.qr_format_trust_device` is on. The live config is `{self_g: 0, qr_format_trust_device: 0, logo_qr: 0}`, so the feature is **off**, and the confirm URL is used unchanged. [O]

## Open questions

- [U] The exact body of a successful `chk_wait_scan` and `chk_wait_cfirm` response (field names, and whether the session data is in the body as well as in `Set-Cookie`).
- [U] Whether `pc-status` returns `18012` both before and after the scan, or uses a different code once scanned.
- [U] The trust-device proof algorithm, needed only if the server turns `qr_format_trust_device` on.
- [U] Whether `logged_uids` should be `[]`, empty or `null`. The password flow sends `null`.
- [U] What happens when `qr_retry` runs out: does the client request a new QR automatically?
