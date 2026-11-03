# Call signaling

How a Zalo PC/Web client sets up, rings, answers and tears down 1-1 and group voice/video calls: the `/api/voicecall/*` HTTP endpoints, the `extendData` media description, how call events reach the client, and the UDP relay registration ("ZRTP" in Zalo's naming) that precedes media. The media packets themselves are in [media-transport.md](media-transport.md).

Evidence tags: `[O]` observed live, or read verbatim from Zalo's own JS bundle / binary strings; `[I]` implemented in the previous client, or derived from disassembly of Zalo binaries (`ZaloCall.exe`, Android `libzrtc.so`), with no recorded live confirmation; `[U]` unknown. Reference build: Zalo PC 26.10.11 (renderer + `ZaloCall.exe`), some native findings from 26.10.20 and Android 26.10.02.

## 1. Architecture of the official client

- Zalo PC splits calls between the Electron renderer (`[zcall-v2] call-helper`) and a native engine `plugins/capture/ZaloCall.exe`. The renderer does all HTTP signaling; the native engine does relay registration, media and the call state machine. [O]
- The engine asks the renderer to send a signal with `sendSignal(<signal id>, body)`; the renderer maps the id to an endpoint, sends it, and returns the decrypted `data` via `recvSignal`. [O]
- Incoming call control events (`act_type == "voip"`) are forwarded by the renderer to the engine as `{ "type": "control", "data": <event> }`. [O]
- A rewrite does not need the split; it only needs the wire behaviour below. The renderer↔engine bridge is documented in §9 for reference.

## 2. Transport of signaling requests

- **Host:** service key `voice_call` from the login service map (`zpw_service_map_v3.voice_call[0]`), default `https://voicecall-wpa.chat.zalo.me`. [I]
- **Method:** `GET {voice_call}/api/voicecall/<op>?zpw_ver=<v>&zpw_type=<t>&params=<enc>`. [O]
- **Encoding:** `params` = session AES (`encodeAES` with the login-derived key, `zpw_enk`) of the JSON body, base64, URL-encoded. No `signkey`. Response is the usual `{error_code, error_message, data}` envelope; `data` is decrypted with the same key. See [../security/payload-encryption.md](../security/payload-encryption.md) and [../auth/session.md](../auth/session.md). [O] (captured requests decrypt with this key byte-exactly.)
- **Version params:** the captured official desktop sends `zpw_ver=693&zpw_type=24` on voicecall endpoints while the rest of its API uses `694&30`. Whether the backend keys any behaviour on this pair is `[U]`. [O]
- **`data` normalisation:** `data` may come back as an object, a JSON string, an empty string or null. The native engine requires a JSON object, so parse strings and treat empty/null as `{}`. [I]
- **`imei` field:** the official PC `imei` is `uuid_v4() + "-" + md5_hex(userAgent)` (69 chars). [O] A 24-hex legacy imei was reported to get degraded invites (`params.status=5`, `extendData` stripped). [I]
- **Ids:** `calleeId` / `callerId` / `toId` are the 64-bit decimal user ids (19 digits). The same users also have a **32-bit id** used inside UDP packets (`fromId`, `toId`, `uidFrom` in events). Passing a 32-bit id where a 64-bit one is expected returns `114`. [O] The 32-bit id of the logged-in user is the second dot-separated component of `zpw_sek`, and equals the low 32 bits of the 64-bit id cast to `uint32` (`std::stoull` → `uint32_t`). [I]

## 3. Signal → endpoint table

Signal ids are the engine's ids; command ids come from the renderer's endpoint registration table `W(<cmd>, ...)`. [O]

| Signal | Cmd | Endpoint | Purpose |
|---|---|---|---|
| 401 | 11300 | `/api/voicecall/requestcall` | Allocate call record, get relays |
| 416 | 11301 | `/api/voicecall/request` | Publish caller media, ring callee |
| 407 | 11302 | `/api/voicecall/ringring` | Callee: "I am ringing" |
| 402 | 11303 | `/api/voicecall/answer` | Callee: accept / reject |
| 408 | 11304 | `/api/voicecall/answerack` | Caller: ack the answer |
| 405 | 11305 | `/api/voicecall/cancel` | Caller: cancel before answer |
| 409 | 11306 | `/api/voicecall/endcall` | Either side: hang up a connected call |
| 411 | 11307 | `/api/voicecall/holdreq` | Hold request |
| 412 | 11308 | `/api/voicecall/holdresp` | Hold response |
| 413 | 11309 | `/api/voicecall/unholdreq` | Unhold request |
| 414 | 11310 | `/api/voicecall/unholdresp` | Unhold response |
| 406 | 11311 | `/api/voicecall/logendcall` | Post-call log |
| 12006 | — | `/api/voicecall/conf` | Camera/mic on-off (`sendOnOffCamMic`) |
| 12056 | 12056 | `/api/voicecall/requestaudio2video` | Registered, but **no** `case 12056` in the signal dispatcher |
| 12064 | — | `/api/voicecall/requestsharescreen` | Screen share request |
| 12433 | — | `/api/voicecall/group/requestcall` | Group: create call |
| 12434 | — | `/api/voicecall/group/request` | Group: publish media / ring |
| 12435 | — | `/api/voicecall/group/adduser` | Group: add member |
| 12436 | — | `/api/voicecall/group/answer` | Group: answer |
| 12437 | — | `/api/voicecall/group/cancel` | Group: cancel |
| 12438 | — | `/api/voicecall/group/endcall` | Group: leave/end |
| 12439 | — | `/api/voicecall/group/ringring` | Group: ringing |
| 12445 | — | `/api/voicecall/group/state` | Group: state query |
| 12446 | — | `/api/voicecall/group/finish` | Group: finish log |
| 12447 | — | `/api/voicecall/group/notify` | Group: notify |
| 12097 | — | `/api/voicecall/group/ping` | Group: member state ping |
| 12098 | — | `/api/voicecall/group/broadcast` | Group: member state broadcast |
| 12043 | — | `/api/voicecall/group/join` | Group: join |
| 12044 | — | `/api/voicecall/group/sharescreen` | Group: screen share |

The 1-1 rows (401..414, 406) and 12006/12056 are `[O]`; the group rows and 12064 are `[I]` (signal ids and bodies taken from the renderer's `sendSignal` handler, never confirmed by a successful live group call).

## 4. Call types and media description

### 4.1 Call type codes

`makeCall` computes `type = isGroup ? (isVideo ? 6 : 5) : (isVideo ? 3 : 1)`: 1 = 1-1 audio, 3 = 1-1 video, 5 = group audio, 6 = group video. [O] On the group wire (`typeRequest` / `callType` of group endpoints) the engine sends `0` for group voice and `1` for group video. [I]

### 4.2 `codec` (string)

A JSON array serialised to a string (the official client appends `\n`). Audio entry: [I]

```json
[{"dynamicFptime":0,"frmPtime":20,"name":"opus/16000/1","payload":112}]
```

Entry keys read by the engine: `name`, `payload`, `frmPtime`, `dynamicFptime` (log format `[codecName = %s, payload = %d, frameTime = %d, dynamicFPT = %d]`). [O]

### 4.3 `extendData` (string)

A JSON object serialised to a string (trailing `\n`). The native `_parseExtendData` (`callcontroller.cpp`) reads: `callType`, `video.codec`, `newZrtc`, `packetMode` (default 2), `gccMode`, `gccAudio`, `gccEarlyCall`, `gccSVLR`, `srtpMode`, `srtcp`, `sP2P`, `maxFT`, `platform`, `fecTP`, `supportHevcDecode`, `tpType`, `spTcp`, `serverAddr`, `select2side`, `extraInfo`, `serverResult`. [O] (`numServers` exists as a string in the binary but was not seen in that function.)

Shape published by the previous client (mirrors the official one as far as known): [I]

| Key | Value sent | Meaning |
|---|---|---|
| `callType` | 0 audio / 1 video | |
| `newZrtc` | 1 | New relay protocol |
| `packetMode` | 2 | `[U]` meaning |
| `srtpMode`, `srtcp` | 0, 0 | No SRTP (RTP observed in plaintext, see §8) |
| `gccMode`, `gccAudio`, `gccSVLR` | 1, 1, 1 | Congestion control flags |
| `gccEarlyCall`, `fecTP`, `sP2P`, `tpType` | 0 | `sP2P:0` forces the relay path |
| `spTcp` | 1 | |
| `maxFT` | 60 | Max frame time (ms) |
| `platform` | 2 | PC |
| `supportCallBusy` | 1 | |
| `supportHevcDecode` | 0 | 0 = cannot decode HEVC. A non-zero value lets the peer switch its encoder to H.265 (see [media-transport.md](media-transport.md) §5.4) |
| `video` | `{"codec":[{"name":"h264","payload":97}]}` | |
| `numServers` | number of relays probed (caller) / 0 (callee) | |
| `p2p` | `[{ip,port,type}]` | `type:0` = local LAN address, `type:1` = public address learned from the relay's InitZRTP reply (same port) |
| `serverAddr` | `[{rtp,rtcp,rtpIPv6,rtcpIPv6,tpType}]` | Every relay from the 401 response |
| `serverResult` | `[{rtp,rtcp,rtpIPv6,rtcpIPv6,recv,rtt,spTcp,tpType}]` | Echo probe result per relay: packets received and RTT (ms) |
| `select2side` | 1 | Callee only |

Peer `extendData` observed in a live call (2026-10-30, Zalo mobile/desktop peer): `supportHevcDecode:4`, `packetMode:2`, `srtpMode:0`, `srtcp:0`, `negoVidQual:1`, `maxBitratePC:800`, `minBitratePC:150`, `video.codec=[{h264, payload:97}]`. [O]

The official callee logs `_doInitZRTP size = N`; a healthy call reaches `size = 4`. When an invite advertised only one relay/p2p entry, the official callee logged `size = 1` and tore the call down within ~1 s. Every relay from the 401 must therefore be listed in `serverAddr`/`serverResult`. [O] Whether `size` counts `serverAddr` entries or `p2p` entries is `[U]`.

### 4.4 `session`

An opaque string shared by both legs and echoed in UDP InitZRTP/ping/end packets. The 401 response carries `sessId`; captured official sessions are 154 url-safe base64 characters. [O] Whether the caller must mint its own `session` or reuse `sessId` from the 401 is contradictory in the old notes (see Open questions). The callee receives it as `params.sessId` in the invite and echoes it as `session_id` in 402. [I]

## 5. 1-1 endpoints

All bodies are plaintext before session AES encryption.

### Request call (401)
- **Request:** `GET {voice_call}/api/voicecall/requestcall` (default host: `https://voicecall-wpa.chat.zalo.me`)
- **Encoding:** session AES (`zpw_enk`), no signkey
- **Params:**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `calleeId` | string (64-bit id) | yes | Callee |
  | `callId` | int | yes | Client-proposed id; previous client used `(now_ms % 1_800_000_000) + 100_000_000` |
  | `codec` | string | yes | `"[]"` on the bare dial |
  | `typeRequest` | int | bare dial | Call type (1 or 3). Some probes sent `type` instead `[U]` which is right |
  | `imei` | string | bare dial | §2 |
  | `extendData`, `rtpAddress`, `rtcpAddress`, `session` | string | media variant only | See Notes |
- **Response (`data`):** `id` (server call id, use it from now on), `fromId` (caller 32-bit id), `toId` (callee 32-bit id), `sessId`, `rtpIP`, `rtcpIP`, `servers: [{rtpaddr, rtcpaddr, rtpIPv6, rtcpIPv6, spTcp, tpType}]`, `status` (0 = ok), `msg`. Relays observed on UDP port 4200, RTP and RTCP on the same `ip:port`. [O]
- **Errors:** `status != 0` with `msg` = refused. `error_code 114` = calling a non-friend / bad id form. [O]
- **Notes:** The official body shape is disputed. One recorded observation says the desktop's 401 has exactly seven keys `{callId, calleeId, codec, extendData, rtcpAddress, rtpAddress, session}` using a relay cached from the previous call, and that extra keys (`typeRequest`, `imei`, `subCommand`) degraded the invite. A later note says the official 401 only allocates and media goes in 416. The previous client's working path was: bare 401 → relay probe → 416 with media. [I] See Open questions.

### Request / ring (416)
- **Request:** `GET {voice_call}/api/voicecall/request`
- **Encoding:** session AES
- **Params:**
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `calleeId` | string | yes | |
  | `callId` | int | yes | `id` from 401 |
  | `codec` | string | yes | §4.2 |
  | `rtpAddress`, `rtcpAddress` | string `ip:port` | yes | Chosen relay (first relay that answered InitZRTP, else first in list) |
  | `session` | string | yes | §4.4 |
  | `imei` | string | yes | |
  | `extendData` | string | with media | §4.3 |
  | `subCommand` | int `3` | with `extendData` | Must be added together with `extendData` |
- **Response:** `status` (0 = ok), `msg`.
- **Notes:** Without `subCommand:3` the record looks like a legacy request: the callee still gets an invite but its `answer` fails with `-101`. [I] A 416 reaching the official engine after its call record died logs `not dialing` (proves 416 is delivered to the callee engine). [O]

### Ringing (407)
- **Request:** `GET {voice_call}/api/voicecall/ringring`
- **Params:** `callerId` (64-bit `uidN` from the invite; 32-bit `uidFrom` gets `114`), `callId`, `status: 0`, `imei`. [O for the `114` rule]
- **Notes:** The official callee rings back before answering. [I]

### Answer / reject (402)
- **Request:** `GET {voice_call}/api/voicecall/answer`
- **Params:** official renderer `sendAnswerCall(callerId, callId, status, codec, extendData, rtcpAddress, rtpAddress, session_id)`; keys whose value is `undefined` are dropped (JSON.stringify). [O]
  | Field | Type | Required | Meaning |
  |---|---|---|---|
  | `callerId` | string 64-bit | yes | |
  | `callId` | int | yes | |
  | `status` | int | yes | 0 = accept, 3 = reject (previous client) [I] |
  | `codec` | string | accept | §4.2 |
  | `rtpAddress`, `rtcpAddress` | string | yes | Relay the callee registered with |
  | `session_id` | string | yes | `sessId` from the invite |
  | `extendData` + `subCommand:3` | string, int | accept | Callee `extendData` (§4.3, with `select2side:1`) |
  | `imei` | string | optional | |
- **Errors:** `114` for an empty `rtpAddress`/`session_id` or a 32-bit `callerId`; `111` a required field is missing; `-101` the referenced call/media does not resolve to anything live (e.g. answering a degraded invite). [O]
- **Notes:** The exact official 402 body has never been captured. [U]

### Answer ack (408)
- **Request:** `GET {voice_call}/api/voicecall/answerack`
- **Params:** `calleeId`, `callId`, `status: 0`, `imei`.
- **Notes:** The official caller engine sends it after the answer, but its own receive path has no handler for 408 (`Undefined recv cmd, 408` in its log, harmless). A client must tolerate receiving it. [O]

### Cancel (405)
- **Request:** `GET {voice_call}/api/voicecall/cancel`
- **Params:** `callerId` (the *peer* id, despite the name), `callId`, `callType` (1/3), `status: 0`, `imei`. [I]

### End call (409)
- **Request:** `GET {voice_call}/api/voicecall/endcall`
- **Params:** `uidTo` (peer 64-bit id), `callId`, `status: 3`, `imei`. [I]

### Hold / unhold (411–414)
- **Params:** `{ toId }` only. [I]

### End-of-call log (406)
- **Request:** `GET {voice_call}/api/voicecall/logendcall`
- **Params:** `call_id`, `partner_id`, `status`, `logdata` (string), `duration` (s), `role` (1 caller / 0 callee), `imei`. [I]

### Camera / mic toggle (12006)
- **Request:** `GET {voice_call}/api/voicecall/conf`
- **Params:** `{calleeId, callId, isOnCam: 0|1, isOnMic: 0|1, data}`. [O] (renderer `sendOnOffCamMic`). Content of `data` is `[U]` (previous client sent `""`).
- **Notes:** The peer sees an `onoff_camera` event with `type:1` (on) / `type:0` (off); peer video stopped right after `type:0`. [O] Whether this is also the audio→video upgrade carrier is `[U]` (§7).

### Screen share (12064)
- **Params:** `{uidTo, callId, status}`. [I]

## 6. Group endpoints [I]

Group calls were implemented but have no recorded live success; treat every field here as `[I]`.

- **12433 `group/requestcall`:** `{groupId, callId, typeRequest (0 voice / 1 video), data, partners}`. `data` = JSON string `{extraData:"", groupAvatar, groupId, groupName, maxUsers, noiseId:[member ids]}`; `partners` = JSON string array of member 64-bit ids (excluding self). Response: `status`, `callId`/`id`, `hostCall` (host 32-bit id), `groupId`, `maxUsers`, `partnerIds`, `toId` (array of member 32-bit ids), `params` (JSON string whose keys fill empty top-level ones), `callSetting: {session, servers:[{rtpaddr, rtcpaddr}]}`. The previous client recognised a group SFU by its RTP port 3000 / RTCP port 3500 (1-1 relays use 4200).
- `status: 2` on 12433 was treated as "session not a PC session": re-run `getLoginInfo` with ParamCipher type 24 / client_version 665 (see [../auth/session.md](../auth/session.md)) and retry.
- **12434 `group/request`:** `{calleeId: hostCall, callId, callType, data, session, partners, groupId}`; `data` = JSON string `{codec, data: <JSON {extraData, groupAvatar, groupName, hostCall, maxUsers, noiseId}>, extendData, rtcpAddress, rtcpAddressIPv6, rtpAddress, rtpAddressIPv6}`.
- **12435 adduser / 12436 answer / 12437 cancel / 12438 endcall:** `{calleeId, callId, callType, hostCall, data, partners, groupId}` (+ `session` for adduser). Leave with 12438 when connected, 12437 when not.
- **12439 ringring:** `{calleeId, callId, callType, hostCall, data, partners}`.
- **12445 state:** `{callId, hostCall, callType}`. **12446 finish:** `{callId, callType, data, duration, hostCall, partners, status, joinTime}`. **12447 notify:** `{callId, hostCall, callType, status, data}`.
- **12097 ping / 12098 broadcast:** `{callId, hostCall, userId, callstate, audiostate, videostate, params}`. **12043 join:** `{userId, callId, hostCall, status, srcJoinCall, audiostate, videostate, data}`. **12044 sharescreen:** `{userId, callId, hostCall, status, audiostate, videostate, data}`.
- The host keeps the room open after others leave (`ZaloCall.exe` behaviour per disassembly).

## 7. Incoming call delivery

### 7.1 Carrier

- **Live invites arrive only over the control WebSocket** (see [../realtime/websocket.md](../realtime/websocket.md)): commands `601` (push ctrl), `602` (ctrl action), `603` (pull ctrl), `604` (pull voip). The official desktop subscribes to queues `603_1`, `603_0`, `604_0`, and the full media-carrying `act:"request"` invite arrives there. [O] Pull only queues the server listed in the auth payload (`ctrl_qCmds`): pulling an unlisted queue such as `604_0` closes the socket with `3002 NOT_FOUND_WORKER`. [O]
- **HTTP long-polling** (`zpw_ctrs_new[0]`, e.g. `p4-ctl-chat.zaloapp.com`, `getControllFromPolling`) only carries durable records: a polling-only client sees the ~716-byte `act:"answer"` stub with `status:5`, never the live `act:"request"`. [O] Polling is still useful for answers/ends while the socket reconnects. Its request: `GET {lp_control}/?zpw_ver&zpw_type&params=<lastActionId>,<lastActionIdOther>,<urlencode(base64(JSON queues) + ",1")>&ts=<s>`, where `queues = {"<qid>": {"lastId": "...", "ids": [...]}}` (the previous client used qids `4,5,14,17,18,19,20`). The response (session-AES) carries `resetLastActionId`, `lastActionId`, `queueStatus{qid:{lastId, ids}}` and `controls[]`. The server re-delivers an entry until its id is echoed back in `ids` (keep at most 10, ascending). [I] Full details belong in [../realtime/events.md](../realtime/events.md).

### 7.2 Event shape

Each `controls[].content` with `act_type == "voip"` is a call event; its `data` is a JSON **string** to parse. [O] Fields used (all `[I]` except where noted):

| Field | Meaning |
|---|---|
| `act` | `request`, `group_request`, `ring_ring`, `group_ring_ring`, `answer`, `group_answer`, `cancel`, `group_cancel`, `end_call`, `group_end_call`, `group_broadcast` |
| `data.callId` / `data.id` | Call id |
| `data.uidN` | Peer 64-bit id (use for `callerId`) [O] |
| `data.uidFrom` / `fromId` | Sender 32-bit id [O] |
| `data.uidTo` / `toId` / `calleeId` | Receiver 32-bit id |
| `data.ts` | Event time, seconds or milliseconds (values < 10^10 are seconds) |
| `data.status` | See below |
| `data.callType`, `data.video.enable` | Call type / video flag |
| `data.Dname`, `data.groupName` | Display name |
| `data.extraData` | JSON string; for `group_answer` its `status` overrides `data.status` |
| `data.callState`, `data.userId` | `group_broadcast`: `callState 3` = member joined |
| `data.params` | JSON **string** with the media handshake: `sessId`, `rtpIP`, `rtcpIP`, `rtpSerIp`, `codec`, `extendData`, `Dname`, `status`, `zrtc_config{callType, video{enable}}`, `reveiverId` (sic) / `receiverId` [O for nesting in `params`] |

### 7.3 Invite rules

- A media-ready invite is identified by a non-empty `params.extendData`. `params.status` is `5` on every fresh invite, including complete ones, so do not use it as the readiness test. [O] A degraded invite (`status=5`, empty `extendData`) cannot be answered (`402` → `-101`). [O]
- A `request` with the same `callId` may arrive more than once (the server upgrades the record in place); ignore it once the call is set up. [I]
- Drop invites older than ~90 s (clock skew between phone and PC is common); on the very first poll after startup keep only invites younger than ~45 s. [I]
- For a video invite, any of: `callType` 3 or 6, `extendData.callType == 1`, `extendData.video` present, `zrtc_config.video.enable == 1`. [I]
- An `answer` event with no local call, from the peer, carrying `status 5` or (`status 0/3` with media) was observed to be the only form some invites took; the previous client re-dispatched it as a `request`. [I]
- `answer` status values handled by the caller: `0` accepted; `1..4` busy / declined; `5` interrupted or declined. A `status 5` answer that arrives with `extendData` within ~8 s of dialing is a stub and must be ignored. [I]

## 8. Relay registration (ZRTP) and key exchange

"ZRTP" is Zalo's name for its UDP relay protocol (`ZRTPPacket` in `libzrtc.so`); it is **not** RFC 6189 key agreement. No key exchange was observed: `srtpMode:0`/`srtcp:0` are advertised by both sides and captured RTP/RTCP were plaintext. [O]

### 8.1 Common header (21 bytes, little-endian)

| Off | Size | Field |
|---|---|---|
| 0 | 1 | Direction: `0x01` request (client→relay), `0x02` response |
| 1 | 1 | Packet type: `0x7E` init / group control, `0x01` ping / echo / end / in-call control (`0x7F` seen in responses) |
| 2 | 4 | `seq` |
| 6 | 4 | `subSeq` |
| 10 | 4 | `fromId` (own 32-bit id) |
| 14 | 4 | `token` (relay token once known, else 0) |
| 18 | 2 | `cmd` |
| 20 | 1 | `subCmd` |

Strings in bodies are `u16 LE length + bytes` unless stated. [I] (layout from `ZaloCall.exe` disassembly; the packets below did produce live relay replies and media flow.) [O] for "relay accepts these and returns a token".

### 8.2 Commands

| Packet | Type | Cmd | Sub | Body |
|---|---|---|---|---|
| InitZRTP, caller | `0x7E` | `0x000B` | see below | `callId u32` · `partnerId u32` · `sessId (u16 len)` |
| InitZRTP, callee | `0x7E` | `0x000C` | see below | same |
| Echo probe | `0x01` | `0x0005` | 0 | header `seq` = probe seq, `fromId = 0x003E9765`, `token = 0x012F833C` (fixed magic); body = decimal unix-seconds string (u16 len) |
| Ping / keepalive | `0x01` | `0x0002` | 1 | `token` in header; body = `sessId` (u16 len) |
| End call | `0x01` | `0x0020` | `0x09` | `sessId` with **u32** length prefix |
| Group InitZRTP | `0x7E` | `0x000B` | 4 | `callId u32` · `groupId u32` · `sessId (u16)` · `u16 4` · `00 03 01 01` |
| Group InitZRTP alt | `0x7E` | `0x000C` | 6 | `callId u32` · `00 03 00` · `sessId (u16)` · `u16 4` · `00 03 01 01` |
| Group ping | `0x7E` | `0x0033` | 1 | `callId u32` · `groupId u32` · `sessId (u16)` · `u16 3` · `00 03 01` |
| Group end | `0x7E` | `0x0003` | 1 | `callId u32` |
| Group layers / subscribe / room-updated | | `0x0020` / `0x0032` | `0x0C` / `0x0D` | See [media-transport.md](media-transport.md) §8 |

All `[I]` (disassembly), with InitZRTP/echo/ping 1-1 forms confirmed by live replies `[O]`.

InitZRTP `subCmd`: `ZaloCall.exe` uses `2` when its `protocol == 3`, which asks the relay to include the client's public `ip|port` in the reply; `1` otherwise. [I] Android `libzrtc.so` sets `subCmd | 4` (e.g. `6`) when the packet is sent from the RTCP socket. [I] The previous client sent `2` from the RTP socket only.

### 8.3 InitZRTP response

`buf[0] ∈ {0x01,0x02}`, `buf[1] ∈ {0x7E,0x7F}`, `cmd ∈ {0x0B,0x0C}`, then: [I] (token/public-address parse worked live [O])

| Off | Field |
|---|---|
| 21 | `res i32` — 0 ok; `-106` invalid call / role |
| 25 | `callId u32` (if `res == 0`) |
| 29 | `token u32` — goes in every later media/ping header from this socket |
| 33 | `u16 len` + `"public_ip|public_port"` (or `ip:port`) when `subCmd` asked for it |

Echo replies come back as `01 01 ...` with `cmd = 0x0005`; count replies and average RTT per relay. [I]

### 8.4 Registration procedure

Official sequence from `voip.log` (caller): `_doInitZRTP size = 0` → `_sendEchoRequestToServers startSeq = 0, endSeq = 120..160, timeout = 2000` → `_handleSelectServersTimeout` → `_doInitZRTP size = 4` → `_handleInitZRTPSuccessful index = 0, size = 4` → `onInitZrtpWithServer`. [O] (`endSeq` varied 120/140/160 across runs.)

Procedure used by the previous client (worked for media): [I]

1. Bind one UDP socket for RTP and one for RTCP (ephemeral ports).
2. Collect relays: response `servers[]` (`rtpaddr`/`rtcpaddr`), `callSetting.servers[]`, `extendData.serverAddr[]`, then top-level `rtpIP`/`rtpAddress`/`rtpSerIp` + `rtcpIP`/`rtcpAddress`; dedupe by RTP address.
3. To every relay from the RTP socket: InitZRTP + 7 echo probes, twice, 20 ms apart (14 probes per relay; the official setting is `checkPkgRec = 14`).
4. Listen ~280 ms. First relay with `res == 0` becomes the selected relay; remember `token` per relay and the first public address.
5. On each successful init, send one ping (1-1) or group ping + layers + subscribe (group) to that relay.
6. Build `extendData` (`serverResult` from step 4; relays that did not answer were reported as `recv 14, rtt 25` by the previous client [I]) and send 416 / 402.
7. Keepalive every 400 ms: a ping with that relay's token to every relay; while no token yet, resend InitZRTP + one echo instead.
8. On hang-up: send the end packet to every relay.

Single-port relays (RTP addr == RTCP addr, the common case on port 4200): the relay appears to bind the client endpoint to the source port of the most recent init/ping. Sending pings from the RTCP socket to the same `ip:port` diverted ~70% of incoming RTP to the RTCP socket. Send pings only from the RTP socket when the addresses are equal, and accept RTP on both sockets anyway. [I] (root-caused from a capture; fix not live-verified.)

### 8.5 Relay switch on answer

The callee's `answer` event `params.rtpSerIp` (or `rtpIP`) names the relay the call will use. The caller switches its media target to that address and to the token it got from that relay during step 4. [I]

## 9. Call state machine

### 9.1 Caller

```
UI dial
  → 401 requestcall           (allocate; relays, sessId, 32-bit ids)
  → relay registration        (§8.4)
  → 416 request (+media)      server pushes the invite to the callee
  ← event ring_ring           peer is ringing
  ← event answer status=0     → switch relay (§8.5), start media, 408 answerack
  ← event answer status≠0     → declined / busy; stop
  ... in call: 12006 conf for cam/mic, 411–414 hold
  hang-up: not answered → 405 cancel ; answered → 409 endcall + 406 logendcall
  ← event cancel / end_call   → stop media, send UDP end
```

Official native caller trace, condensed: `request make call 1-1` → `[SIG]requestCall` → `[SIG]receive 401 resp` → `_initZrtcConfig 1..4` → `_initAudioDevice` → `setCallState state = 1` → relay init (§8.4) → `[SIG]receive 416 resp` → `[SIG]receive ring` → `setCallState state = 3` → `[SIG]receive answer` → `state = 4` → `state = 5` → `Call to confirmed. _handleStartCall` → `onCallUpdateP2PStatus` → media up. [O] Full trace and state codes: [voip-log.md](voip-log.md).

### 9.2 Callee

```
← invite (act request, media-ready)
  → 407 ringring (immediately)
  → relay registration as callee (InitZRTP cmd 0x0C) using params relays and sessId
  user accepts → 402 answer status 0 (+ callee extendData, subCommand 3) → start media
  user rejects → 402 answer status 3
← event cancel / end_call → stop
hang-up in call → 409 endcall + 406 logendcall
```

Official callee: `recv incoming call req 1-1` → `[SIG]recvIncomCall` → show window → accept: `UiAnswerCallEvent, 1` → `incomingCall isVideo = 0` → `_initZrtcConfig 1..4` → `setCallState state = 2` → `_doInitZRTP size = N` → … [O]

### 9.3 Multi-device / group leg handling [I]

The previous client dialed every group member with its own 401/416 leg (call ids `callId + 10·i`), promoted the first leg to answer to primary, and kept the rest as parallel media legs. Whether Zalo expects this or only the 12433/12434 room flow is `[U]`.

## 10. Renderer ↔ `ZaloCall.exe` bridge (reference)

Only needed to drive the official engine. [O] for names/constants read from the bundle; frame round trip `[I]`.

- Integrity: the main process checks `ZaloCall.exe` MD5 against `CALL_NATIVE_HASH` (26.10.11: `a2b920c00115ba11a7fcdcb642147e9c`).
- Pipes: `\\.\pipe\PipeZCallRecv` (engine → app) and `\\.\pipe\PipeZCallSend` (app → engine).
- Frame: AES-128-CBC, PKCS#7, key = base64 `yjAF9oqMWl6XfXYJn9mA7w==`, IV = 16 zero bytes; ciphertext hex-encoded, frames terminated by `$`.
- `init`/`updateLocal` payload (`getInitialData`): `{type:"update", command, data:{local:{id (decimal 64-bit uid; engine parses with stoull), avatar, name}, osInfo, mainWindowId, logPath, zrtcLogPath, dumpPath, clientVersion: 694, language, timeoutMakeCall2ZRtp: 15000, enablePreviewDeviceOutgoing, enablePreviewDeviceIncall, enableUnstableConnecting, receiveCallWhileNoMicrophoneDetected, receiveCallWhileNoSpeakerDetected, autoAudioInput, autoAudioOutput, defaultAudInputDevice, defaultAudOutputDevice, defaultVidDevice}}`.
- `makeCall` request: `{type:"request", command:"makeCall", data:{partner:[{id, avatar, name}], type: <call type §4.1>, groupInfo: {id:"group_<gid>", name, avatar} | null, isFirstCall}}`.
- UI action log ids seen: accept `1150311` (`sendAction: 5`), reject `1150303` (`sendAction: 8`), cancel while dialing `1150201`. [O]

## Open questions

- [U] Which side mints `session`: the 401 response `sessId`, or a client-generated 154-char url-safe base64 value sent in both 401 and 416? The old notes assert both.
- [U] Official 401 body: bare `{calleeId, callId, codec:"[]", typeRequest, imei}`; seven-key media 401 with a cached relay; or a bare 401 followed by a second media 401 and then 416. Three conflicting accounts exist. Also whether the type key is `typeRequest` or `type`.
- [U] The exact official 402 body (never captured), and the full meaning of `status` values in 402 and in `answer` events (especially `5`).
- [U] Whether `_doInitZRTP size` counts `serverAddr` entries or `p2p` entries.
- [U] InitZRTP `subCmd` per socket (`1`, `2`, `6`) and whether the relay needs an init from the RTCP socket at all.
- [U] Meaning of `packetMode:2`, `negoVidQual`, `supportHevcDecode:4` vs `1`, `select2side`, `maxFT`.
- [U] Whether `zpw_ver=693&zpw_type=24` on voicecall endpoints is required.
- [U] Who calls `requestaudio2video` (12056) and whether the audio→video upgrade uses `conf` (12006) with `isOnCam=1`; content of `conf.data`.
- [U] Group calls: none of §6 or the group UDP packets has a recorded live success.
